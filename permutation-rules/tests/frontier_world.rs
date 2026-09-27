//! Frontier (rules v10) world kernels: geometry, terrain, holdings, hosts,
//! stances, the clash, sieges and travel (design §3, §5.1, §6), and the
//! §9.4 property tests that concern them: determinism and order
//! independence of the clash, the roster freeze, and lag invariance.

use permutation_rules::fixed::{BPS_ONE, MILLI};
use permutation_rules::frontier::clash::{
    clash_seed, frontier_ruleset, resolve_clash, ClashInput, ClashOutcome, Fate, Fighter, Garrison,
    Relations,
};
use permutation_rules::frontier::geometry::*;
use permutation_rules::frontier::holding::{
    duplicate_cost, shield_secs, Accrual, Effect, Holding, Resource, Tier, DAY, HOUR,
};
use permutation_rules::frontier::host::{
    rout_survivors, Host, HostError, Presence, Stamina, ENGAGE_STAMINA, MAX_HOST_TROOPS,
    MIN_HOST_TROOPS, STAMINA_CAP,
};
use permutation_rules::frontier::siege::{
    auto_reinforce, may_besiege, raid, required_bells, BellReport, Donor, HoldingKind, Relation,
    Siege, SiegeCheck, SiegeRefusal, SiegeStatus, Vigil, VigilError,
};
use permutation_rules::frontier::stance::{
    damage_bps, payoff_bps, posture_of, Posture, Stance, STANCES,
};
use permutation_rules::frontier::terrain::{generate_province, ProvinceTerrain, GATES};
use permutation_rules::frontier::travel::*;
use permutation_rules::frontier::RULES_VERSION_FRONTIER;
use permutation_rules::hash::sha256;
use permutation_rules::hex::{hexes_within, Hex};
use permutation_rules::map::Terrain;
use permutation_rules::params::RULES_VERSION;
use permutation_rules::rng::Seed;
use permutation_rules::units::UnitType;

// ------------------------------------------------------------ helpers

/// splitmix64: a tiny deterministic generator for the property tests.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            v.swap(i, j);
        }
    }
}

fn seed(tag: &[u8], n: u64) -> Seed {
    sha256(&[tag, &n.to_le_bytes()])
}

const UNITS: [UnitType; 7] = [
    UnitType::Spearman,
    UnitType::Archer,
    UnitType::Horseman,
    UnitType::Pikeman,
    UnitType::Crossbowman,
    UnitType::Knight,
    UnitType::Scout,
];

fn fighter(id: u64, faction: u8, unit: UnitType, troops: u32, tile: u8) -> Fighter {
    Fighter {
        id,
        faction,
        unit,
        troops: troops * 1000,
        stamina: STAMINA_CAP,
        tile,
        posture: Posture::default(),
        retreat_bps: None,
        dealt_bps: BPS_ONE,
    }
}

/// A flat, all-plains province (for clashes that should not depend on
/// generated land).
fn flat() -> ProvinceTerrain {
    ProvinceTerrain {
        terrain: [Terrain::Plains; PROVINCE_TILES],
        resource: [None; PROVINCE_TILES],
        sites: [0; SITES_PER_PROVINCE],
        site_count: 0,
    }
}

const P: ProvinceCoord = ProvinceCoord { p: 3, q: 1 };

fn clash(
    terrain: &ProvinceTerrain,
    bell: u32,
    residents: &[Fighter],
    garrisons: &[Garrison],
    arrivals: &[Fighter],
    relations: Relations,
) -> ClashOutcome {
    resolve_clash(
        &frontier_ruleset(),
        &ClashInput {
            province: P,
            bell,
            seed: seed(b"bell", bell as u64),
            terrain,
            residents,
            garrisons,
            arrivals,
            relations,
        },
    )
    .expect("valid clash")
}

// ------------------------------------------------------------ versions

#[test]
fn v10_lands_beside_v9() {
    assert_eq!(RULES_VERSION, 9, "v9 (Skirmish) rules are unchanged");
    assert_eq!(RULES_VERSION_FRONTIER, 10);
}

// ------------------------------------------------------------ geometry (§3.1)

#[test]
fn provinces_tile_the_plane_and_turn_with_it() {
    for h in hexes_within(150) {
        let p = province_of(h);
        assert!(h.distance(p.centre()) <= PROVINCE_RADIUS as u32, "{h:?}");
        assert_eq!(province_of(h.rotate()), p.rotate(), "{h:?}");
        let (lp, idx) = locate(h);
        assert_eq!(lp, p);
        assert_eq!(lp.tile(idx), Some(h));
    }
    for i in 0..provinces_within(12) {
        let p = ProvinceCoord::from_index(i);
        for t in 0..PROVINCE_TILES as u8 {
            assert_eq!(province_of(p.tile(t).unwrap()), p);
        }
    }
}

#[test]
fn rings_wedges_and_seats() {
    assert_eq!(ProvinceCoord::CONCORD.ring(), 0);
    assert_eq!(ProvinceCoord::CONCORD.wedge(), None);
    for d in 1..=20u32 {
        let ring = ring_provinces(d);
        assert_eq!(ring.len() as u32, 6 * d);
        for k in 0..6u8 {
            let n = ring.iter().filter(|p| p.wedge() == Some(k)).count() as u32;
            assert_eq!(n, d, "ring {d} wedge {k}");
        }
        for p in &ring {
            assert_eq!(p.rotate().ring(), d);
            assert_eq!(p.rotate().wedge(), Some((p.wedge().unwrap() + 1) % 6));
        }
    }
    for f in 0..6u8 {
        let s = seat_of(f);
        assert!(s.is_seat());
        assert_eq!(s.wedge(), Some(f));
    }
    // Capacity table (design §3.4): sites at R_MAX.
    for (r, sites) in [
        (24u32, 21_528u32),
        (32, 37_944),
        (48, 84_600),
        (64, 149_688),
    ] {
        let settleable = provinces_within(r) - provinces_within(1);
        assert_eq!(settleable * SITES_PER_PROVINCE as u32, sites, "R_MAX {r}");
    }
}

#[test]
fn heartlands_are_rings_two_and_three_of_the_own_wedge() {
    for i in 0..provinces_within(6) {
        let p = ProvinceCoord::from_index(i);
        for f in 0..6u8 {
            let expect = (p.ring() == 2 || p.ring() == 3) && p.wedge() == Some(f);
            assert_eq!(is_heartland(p, f), expect);
        }
    }
}

#[test]
fn marches_partition_the_provinces() {
    let mut seen = std::collections::BTreeMap::new();
    for i in 0..provinces_within(15) {
        let p = ProvinceCoord::from_index(i);
        let m = march_of(p);
        let members = march_members(m);
        assert!(members.contains(&p));
        let mut sorted = members.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), MARCH_PROVINCES);
        for q in members {
            assert_eq!(march_of(q), m);
        }
        let mr = march_of(p.rotate());
        assert_eq!(march_members(mr)[0], members[0].rotate());
        *seen.entry(m).or_insert(0) += 1;
    }
    assert!(seen.values().all(|n| *n <= MARCH_PROVINCES));
}

#[test]
fn coordinates_are_bounded() {
    let b = COORD_BOUND;
    assert!(check_hex(Hex::new(b, -b)).is_ok());
    assert!(check_hex(Hex::new(b + 1, 0)).is_err());
    assert!(check_hex(Hex::new(0, -b - 1)).is_err());
    // no overflow at the corners of the box
    for h in [
        Hex::new(b, b),
        Hex::new(-b, -b),
        Hex::new(b, -b),
        Hex::new(-b, b),
    ] {
        let p = province_of(h);
        assert!(h.distance(p.centre()) <= 4);
    }
}

#[test]
fn regions_cover_all_sixteen() {
    let mut n = [0u32; REGIONS as usize];
    for i in 0..provinces_within(20) {
        n[region_of(ProvinceCoord::from_index(i)) as usize] += 1;
    }
    assert!(n.iter().all(|c| *c > 40), "{n:?}");
}

// ------------------------------------------------------------ terrain (§3.2)

#[test]
fn every_wedge_of_a_ring_gets_the_same_land() {
    for s in 0..6u64 {
        let rs = seed(b"ring", s);
        for d in 1..=6 {
            for p in ring_provinces(d) {
                let a = generate_province(&rs, p);
                let b = generate_province(&rs, p.rotate());
                for i in 0..PROVINCE_TILES as u8 {
                    let j = tile_index(tile_offset(i).unwrap().rotate()).unwrap();
                    assert_eq!(a.terrain[i as usize], b.terrain[j as usize]);
                    assert_eq!(a.resource[i as usize], b.resource[j as usize]);
                }
                assert_eq!(a.site_count, b.site_count);
                for k in 0..a.site_count as usize {
                    let j = tile_index(tile_offset(a.sites[k]).unwrap().rotate()).unwrap();
                    assert_eq!(b.sites[k], j);
                }
            }
        }
    }
}

#[test]
fn the_concord_is_six_fold_symmetric() {
    let t = generate_province(&seed(b"ring", 0), ProvinceCoord::CONCORD);
    assert_eq!(t.site_count, 0, "the Concord is neutral");
    for i in 0..PROVINCE_TILES as u8 {
        let j = tile_index(tile_offset(i).unwrap().rotate()).unwrap();
        assert_eq!(t.terrain[i as usize], t.terrain[j as usize]);
    }
}

#[test]
fn every_border_keeps_passable_crossings() {
    // Neighbours in different rings use different ring seeds.
    let ring_seed = |p: ProvinceCoord| seed(b"ring", p.ring() as u64 * 7 + 3);
    for i in 0..provinces_within(5) {
        let p = ProvinceCoord::from_index(i);
        let tp = generate_province(&ring_seed(p), p);
        for g in GATES.iter().flatten() {
            assert!(tp.passable(*g), "{p:?} gate {g}");
        }
        for q in p.neighbors() {
            let tq = generate_province(&ring_seed(q), q);
            let mut crossings = 0;
            for a in 0..PROVINCE_TILES as u8 {
                let ha = p.tile(a).unwrap();
                for hb in ha.neighbors() {
                    let (pb, b) = locate(hb);
                    if pb == q && tp.passable(a) && tq.passable(b) {
                        crossings += 1;
                    }
                }
            }
            assert!(crossings >= 5, "{p:?}–{q:?}: {crossings}");
        }
    }
}

#[test]
fn sites_are_passable_and_two_apart() {
    let mut full = 0;
    let mut n = 0;
    for s in 0..4u64 {
        for i in 1..provinces_within(6) {
            let p = ProvinceCoord::from_index(i);
            let t = generate_province(&seed(b"ring", s), p);
            let sites = &t.sites[..t.site_count as usize];
            for (x, a) in sites.iter().enumerate() {
                assert!(t.passable(*a));
                for b in &sites[x + 1..] {
                    let d = tile_offset(*a).unwrap().distance(tile_offset(*b).unwrap());
                    assert!(d >= 2);
                }
            }
            n += 1;
            full += (t.site_count as usize == SITES_PER_PROVINCE) as u32;
        }
    }
    assert!(full * 10 >= n * 9, "{full} of {n} provinces have 12 sites");
}

#[test]
fn terrain_is_a_pure_function_of_seed_and_province() {
    let p = ProvinceCoord::new(-2, 5);
    assert_eq!(
        generate_province(&seed(b"ring", 9), p),
        generate_province(&seed(b"ring", 9), p)
    );
    assert_ne!(
        generate_province(&seed(b"ring", 9), p),
        generate_province(&seed(b"ring", 10), p)
    );
}

// ------------------------------------------------------------ holdings (§5.1)

#[test]
fn holdings_settle_the_same_whenever_they_are_touched() {
    let mut rng = Rng(7);
    for case in 0..40 {
        let t0 = 1_000_000;
        let mut h = Holding::found(t0, 3, 1);
        h.production[Resource::Food as usize] = 3_000 + rng.below(20_000) as i64;
        h.production[Resource::Wood as usize] = rng.below(9_000) as i64;
        h.upkeep[Resource::Food as usize] = rng.below(15_000) as i64;
        h.upkeep[Resource::Gold as usize] = 1 + rng.below(700) as i64;
        h.touch_owner(t0).unwrap();
        h.enqueue(t0, 3 * HOUR + rng.below(HOUR as u64) as i64, Effect::TierUp)
            .unwrap();
        h.enqueue(
            t0,
            2 * DAY,
            Effect::Production {
                resource: Resource::Gold,
                delta: 2_500,
            },
        )
        .unwrap();
        // run past the start of dormancy (5 days without an owner action)
        let end = t0 + 8 * DAY + rng.below(DAY as u64) as i64;
        let mut once = h.clone();
        once.settle(end).unwrap();
        let mut many = h.clone();
        let mut t = t0;
        while t < end {
            t = (t + 1 + rng.below(40_000) as i64).min(end);
            many.settle(t).unwrap();
        }
        assert_eq!(once, many, "case {case}");
        assert_eq!(once.tier, Tier::Town);
        assert!(once.is_dormant(end));
    }
}

#[test]
fn dormancy_halves_production_from_its_start() {
    let t0 = 0;
    let mut h = Holding::found(t0, 0, 1);
    h.production[Resource::Wood as usize] = 1_000; // 1 unit per hour
    h.touch_owner(t0).unwrap();
    let before = h.stock_at(5 * DAY)[Resource::Wood as usize];
    let after = h.stock_at(5 * DAY + 10 * HOUR)[Resource::Wood as usize];
    assert_eq!(before, 120 * MILLI);
    assert_eq!(after - before, 5 * MILLI);
    assert!(!h.is_released(9 * DAY));
    assert!(h.is_released(10 * DAY));
    let mut second = Holding::found(t0, 0, 2);
    second.touch_owner(t0).unwrap();
    assert!(
        !second.is_released(30 * DAY),
        "only first holdings are released"
    );
}

#[test]
fn accrual_caps_and_floors() {
    let mut a = Accrual::new(900, 1_000 * 3_600, 1_000, 0); // 1000/s, cap 1000
    a.settle(1);
    assert_eq!(a.value, 1_000);
    let mut above = Accrual::new(5_000, 10, 1_000, 0);
    above.settle(3_600 * 50);
    assert_eq!(above.value, 5_000, "loot above the cap is not clipped");
    let mut neg = Accrual::new(100, -3_600, 1_000, 0); // −1 per second
    assert_eq!(neg.settle(150), 50);
    assert_eq!(neg.value, 0);
}

#[test]
fn holding_costs_and_shields() {
    assert_eq!(duplicate_cost(40, 4), 40 * 11 / 2);
    assert_eq!(shield_secs(0), 48 * HOUR);
    assert_eq!(shield_secs(7), 48 * HOUR);
    assert_eq!(shield_secs(8), 72 * HOUR);
    let h = Holding::found(0, 0, 1);
    assert!(h.shielded(47 * HOUR));
    assert!(!h.shielded(48 * HOUR));
    assert_eq!(Tier::Stronghold.worked_radius(), 3);
}

// ------------------------------------------------------------ hosts (§6.1)

#[test]
fn host_limits() {
    assert_eq!(
        Host::muster(1, 1, 0, UnitType::Settler, MIN_HOST_TROOPS, 0),
        Err(HostError::NotAHostUnit)
    );
    assert_eq!(
        Host::muster(1, 1, 0, UnitType::Spearman, MIN_HOST_TROOPS - 1, 0),
        Err(HostError::TooSmall)
    );
    assert_eq!(
        Host::muster(1, 1, 0, UnitType::Spearman, MAX_HOST_TROOPS + 1, 0),
        Err(HostError::TooLarge)
    );
    let mut a = Host::muster(1, 9, 0, UnitType::Knight, 20_000 * 1000, 10).unwrap();
    let b = Host::muster(2, 9, 0, UnitType::Knight, 10_001 * 1000, 10).unwrap();
    assert_eq!(a.merge(&b, 10), Err(HostError::TooLarge));
    let c = a.split(5_000 * 1000, 3).unwrap();
    assert_eq!((a.troops, c.troops), (15_000 * 1000, 5_000 * 1000));
    a.stamina.spend(10, 100).unwrap();
    a.merge(&c, 12).unwrap();
    assert_eq!(a.stamina.at(12), 22, "the lower stamina wins");
    a.route(12);
    assert_eq!(a.troops, rout_survivors(20_000 * 1000));
    assert_eq!(a.troops, 10_000 * 1000);
    assert_eq!(a.stamina.at(12), 0);
    let s = Stamina::full(0);
    assert_eq!(s.at(1_000), STAMINA_CAP);
}

// ------------------------------------------------------------ stances (§6.3)

/// The CI equilibrium check: a 3-cycle plus a neutral Hold, zero-sum, no
/// dominated stance, every pure reply to the uniform cycle worth 0, and
/// withholding (Disarray) strictly worse than revealing any stance.
#[test]
fn stance_table_is_in_equilibrium() {
    let st = |s| Posture::Stance(s);
    for a in STANCES {
        for b in STANCES {
            let (x, y) = (payoff_bps(st(a), st(b)), payoff_bps(st(b), st(a)));
            assert_eq!(x.signum(), -y.signum(), "{a:?} {b:?}");
            if a == Stance::Hold || b == Stance::Hold || a == b {
                assert_eq!(x, 0);
            }
        }
    }
    let cycle = [Stance::Assault, Stance::Flank, Stance::Brace];
    for r in STANCES {
        let v: i64 = cycle
            .iter()
            .map(|o| damage_bps(st(r), st(*o)) as i64 - damage_bps(st(*o), st(r)) as i64)
            .sum();
        assert_eq!(v, 0, "{r:?} vs the uniform cycle");
    }
    for a in cycle {
        for b in cycle {
            if a != b {
                let better_somewhere = STANCES
                    .iter()
                    .any(|o| payoff_bps(st(a), st(*o)) > payoff_bps(st(b), st(*o)));
                assert!(better_somewhere, "{a:?} dominated by {b:?}");
            }
        }
    }
    for committed in STANCES {
        for opp in STANCES.iter().map(|s| st(*s)).chain([Posture::Disarray]) {
            assert!(
                payoff_bps(Posture::Disarray, opp) < payoff_bps(st(committed), opp),
                "withholding {committed:?} against {opp:?} pays"
            );
        }
    }
    assert_eq!(damage_bps(st(Stance::Assault), st(Stance::Flank)), 12_000);
    assert_eq!(damage_bps(Posture::Disarray, st(Stance::Hold)), 6_000);
    assert_eq!(damage_bps(st(Stance::Hold), Posture::Disarray), 12_500);
    assert_eq!(posture_of(false, None), st(Stance::Hold));
    assert_eq!(posture_of(true, Some(Stance::Brace)), st(Stance::Brace));
    assert_eq!(posture_of(true, None), Posture::Disarray);
}

// ------------------------------------------------------------ the clash (§6.3)

/// A random but valid clash scenario on one province.
struct Scenario {
    terrain: ProvinceTerrain,
    residents: Vec<Fighter>,
    garrisons: Vec<Garrison>,
    arrivals: Vec<Fighter>,
    relations: Relations,
}

fn random_scenario(rng: &mut Rng) -> Scenario {
    let terrain = generate_province(&seed(b"ring", rng.below(4)), P);
    let hot = [30u8, 31, 38, 22]; // a few contested tiles
    let tile = |rng: &mut Rng| {
        if rng.chance(80) {
            hot[rng.below(4) as usize]
        } else {
            rng.below(61) as u8
        }
    };
    let mut residents = Vec::new();
    let mut id = 100;
    for _ in 0..rng.below(20) {
        id += 1 + rng.below(5);
        let mut f = fighter(
            id,
            rng.below(4) as u8,
            UNITS[rng.below(7) as usize],
            100 + rng.below(8_000) as u32,
            tile(rng),
        );
        f.stamina = rng.below(121) as u16;
        f.posture = posture_of(rng.chance(50), STANCES.get(rng.below(5) as usize).copied());
        residents.push(f);
    }
    let mut garrisons = Vec::new();
    for (k, t) in [30u8, 22].iter().enumerate() {
        if rng.chance(70) {
            garrisons.push(Garrison {
                id: 9_000 + k as u64,
                faction: rng.below(3) as u8,
                tile: *t,
                troops: rng.below(3_000) as u32 * 1000,
                walls: rng.chance(50),
                posture: posture_of(rng.chance(40), STANCES.get(rng.below(5) as usize).copied()),
            });
        }
    }
    let mut arrivals = Vec::new();
    for _ in 0..rng.below(16) {
        id += 1 + rng.below(5);
        let mut f = fighter(
            id,
            rng.below(5) as u8,
            UNITS[rng.below(7) as usize],
            100 + rng.below(12_000) as u32,
            tile(rng),
        );
        f.stamina = rng.below(121) as u16;
        f.posture = Posture::Stance(STANCES[rng.below(4) as usize]);
        if rng.chance(30) {
            f.retreat_bps = Some(rng.below(30_000) as u32);
        }
        arrivals.push(f);
    }
    let mut relations = Relations::ALL_HOSTILE;
    if rng.chance(50) {
        relations.set_peaceful(0, 1, true);
    }
    Scenario {
        terrain,
        residents,
        garrisons,
        arrivals,
        relations,
    }
}

fn run(s: &Scenario, bell: u32) -> ClashOutcome {
    clash(
        &s.terrain,
        bell,
        &s.residents,
        &s.garrisons,
        &s.arrivals,
        s.relations,
    )
}

/// §9.4 order independence: the outcome is the same for any order of the
/// residents, garrisons and arrivals (reveal order, account order).
#[test]
fn clash_is_deterministic_and_order_independent() {
    let mut rng = Rng(42);
    let mut fought = 0;
    for _ in 0..300 {
        let mut s = random_scenario(&mut rng);
        let a = run(&s, 77);
        assert_eq!(a, run(&s, 77));
        for _ in 0..7 {
            rng.shuffle(&mut s.residents);
            rng.shuffle(&mut s.arrivals);
            rng.shuffle(&mut s.garrisons);
            assert_eq!(run(&s, 77).digest(), a.digest());
        }
        fought += (a.engagements > 0) as u32;
    }
    assert!(fought > 100, "only {fought} scenarios fought");
}

/// The die is drawn from the bell seed: another seed changes some damage,
/// the same seed never does.
#[test]
fn the_die_comes_from_the_bell_seed() {
    let t = flat();
    let r = [fighter(1, 0, UnitType::Spearman, 5_000, 30)];
    let a = [fighter(2, 1, UnitType::Spearman, 5_000, 30)];
    let mut seen = std::collections::BTreeSet::new();
    for b in 0..20 {
        let o = clash(&t, b, &r, &[], &a, Relations::ALL_HOSTILE);
        seen.insert(o.fighter(1).unwrap().troops);
    }
    assert!(seen.len() > 10, "variance collapsed: {seen:?}");
    assert_ne!(
        clash_seed(&seed(b"bell", 1), P, 5),
        clash_seed(&seed(b"bell", 2), P, 5)
    );
}

#[test]
fn stances_move_damage() {
    let t = flat();
    let base = |dp: Posture, ap: Stance| {
        let mut r = fighter(1, 0, UnitType::Spearman, 5_000, 30);
        r.posture = dp;
        let mut a = fighter(2, 1, UnitType::Spearman, 5_000, 30);
        a.posture = Posture::Stance(ap);
        let o = clash(&t, 3, &[r], &[], &[a], Relations::ALL_HOSTILE);
        5_000 * 1000 - o.fighter(1).unwrap().troops
    };
    let hold = base(Posture::Stance(Stance::Hold), Stance::Hold);
    let beaten = base(Posture::Stance(Stance::Flank), Stance::Assault);
    let disarray = base(Posture::Disarray, Stance::Hold);
    assert_eq!(beaten as u64, hold as u64 * 12_000 / 10_000);
    assert!((disarray as i64 - hold as i64 * 5 / 4).abs() <= 1);
}

#[test]
fn fair_share_admits_every_faction() {
    let t = flat();
    // Six residents of faction 0 fill a hex; one faction-1 host arrives.
    let residents: Vec<Fighter> = (0..6)
        .map(|i| fighter(10 + i, 0, UnitType::Spearman, 1_000 + 100 * i as u32, 40))
        .collect();
    let a = [fighter(99, 1, UnitType::Spearman, 150, 40)];
    let o = clash(&t, 1, &residents, &[], &a, Relations::ALL_HOSTILE);
    // faction 1 is guaranteed a slot; the lightest faction-0 host bounces
    assert_ne!(o.fighter(99).unwrap().fate, Fate::Bounced);
    assert_eq!(o.fighter(10).unwrap().fate, Fate::Bounced);
    // On a holding's hex the owner always keeps 3 slots.
    let g = [Garrison {
        id: 500,
        faction: 0,
        tile: 40,
        troops: 1_000_000,
        walls: false,
        posture: Posture::default(),
    }];
    let res: Vec<Fighter> = (0..3)
        .map(|i| fighter(10 + i, 0, UnitType::Spearman, 100, 40))
        .collect();
    let arr: Vec<Fighter> = (0..5)
        .map(|i| fighter(60 + i, 1 + i as u8 % 2, UnitType::Knight, 20_000, 40))
        .collect();
    let o = clash(&t, 2, &res, &g, &arr, Relations::ALL_HOSTILE);
    // admitted hosts fight; hosts bounced at the merge never engage
    let admitted_attackers = arr
        .iter()
        .filter(|f| o.fighter(f.id).unwrap().engaged)
        .count();
    assert_eq!(admitted_attackers, 3);
    for r in &res {
        assert!(o.fighter(r.id).unwrap().engaged, "owner slot {}", r.id);
    }
}

#[test]
fn retreat_ratio_reads_the_frozen_roster() {
    let t = flat();
    let r = [fighter(1, 0, UnitType::Spearman, 10_000, 30)];
    let mut a = fighter(2, 1, UnitType::Spearman, 4_000, 30);
    a.retreat_bps = Some(20_000); // withdraw if defenders > 2 × mine
    let o = clash(&t, 1, &r, &[], &[a], Relations::ALL_HOSTILE);
    assert_eq!(o.fighter(2).unwrap().fate, Fate::Retreated);
    assert_eq!(o.fighter(2).unwrap().troops, 4_000 * 1000, "no loss");
    assert_eq!(o.engagements, 0);
    a.retreat_bps = Some(30_000);
    let o = clash(&t, 1, &r, &[], &[a], Relations::ALL_HOSTILE);
    assert!(o.fighter(2).unwrap().engaged);
}

#[test]
fn damage_ratio_refund_and_the_field() {
    let t = flat();
    let big = fighter(1, 0, UnitType::Knight, 20_000, 30);
    let mut small = fighter(2, 1, UnitType::Spearman, 150, 31);
    small.tile = 30;
    // a friendly faction-1 host on the adjacent tile 31 gives it a fallback
    let friend = fighter(3, 1, UnitType::Archer, 500, 31);
    let o = clash(&t, 4, &[big, friend], &[], &[small], Relations::ALL_HOSTILE);
    let b = o.fighter(1).unwrap();
    assert_eq!(b.stamina, STAMINA_CAP, "ratio ≥ 10 pays no stamina");
    assert_eq!(b.fate, Fate::Stays { tile: 30 });
    let s = o.fighter(2).unwrap();
    assert!(matches!(s.fate, Fate::Destroyed | Fate::Bounced), "{s:?}");
    // an even fight costs stamina to both and one side holds the field
    let a1 = fighter(1, 0, UnitType::Spearman, 5_000, 30);
    let a2 = fighter(2, 1, UnitType::Spearman, 5_000, 30);
    let o = clash(&t, 4, &[a1], &[], &[a2], Relations::ALL_HOSTILE);
    assert_eq!(o.fighter(1).unwrap().stamina, STAMINA_CAP - ENGAGE_STAMINA);
    let stays = o
        .fighters
        .iter()
        .filter(|f| matches!(f.fate, Fate::Stays { .. }))
        .count();
    assert_eq!(stays, 1, "one side holds the field");
}

#[test]
fn losing_residents_fall_back_to_a_friendly_hex() {
    let t = flat();
    let loser = fighter(1, 0, UnitType::Spearman, 3_000, 30);
    let camp = fighter(2, 0, UnitType::Scout, 100, 31); // tile 31 is (0,1)+… adjacent
    assert_eq!(
        tile_offset(30).unwrap().distance(tile_offset(31).unwrap()),
        1
    );
    let a = fighter(3, 1, UnitType::Knight, 20_000, 30);
    let o = clash(&t, 9, &[loser, camp], &[], &[a], Relations::ALL_HOSTILE);
    match o.fighter(1).unwrap().fate {
        Fate::Withdrew { tile } => assert_eq!(tile, 31),
        Fate::Destroyed => {}
        f => panic!("{f:?}"),
    }
}

#[test]
fn allies_share_a_hex_and_neutrals_fight_everyone() {
    let t = flat();
    let mut rel = Relations::ALL_HOSTILE;
    rel.set_peaceful(0, 1, true);
    rel.set_peaceful(2, 6, true); // ignored: NEUTRAL is always hostile
    assert!(!rel.hostile(0, 1) && !rel.hostile(1, 0));
    assert!(rel.hostile(2, 6));
    let r = [fighter(1, 0, UnitType::Spearman, 1_000, 30)];
    let a = [fighter(2, 1, UnitType::Spearman, 1_000, 30)];
    let o = clash(&t, 1, &r, &[], &a, rel);
    assert_eq!(o.engagements, 0);
    assert!(o
        .fighters
        .iter()
        .all(|f| matches!(f.fate, Fate::Stays { .. })));
}

#[test]
fn garrisons_report_the_siege_state() {
    let t = flat();
    let g = [Garrison {
        id: 700,
        faction: 0,
        tile: 30,
        troops: 200 * 1000,
        walls: true,
        posture: Posture::default(),
    }];
    let a = [fighter(5, 2, UnitType::Knight, 25_000, 30)];
    let o = clash(&t, 1, &[], &g, &a, Relations::ALL_HOSTILE);
    let gr = o.garrisons[0];
    assert!(gr.attackers_hold && !gr.defender_present);
    assert!(gr.troops < 200 * 1000);
    let d = [fighter(6, 0, UnitType::Pikeman, 30_000, 30)];
    let o = clash(&t, 1, &d, &g, &a, Relations::ALL_HOSTILE);
    assert!(!o.garrisons[0].attackers_hold);
}

// --------------------------------- roster freeze and lag invariance (§9.4)

/// A resident roster entry of the test province: presence plus state.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    presence: Presence,
    host: Fighter,
    stamina: Stamina,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GarrisonTrack {
    garrison: Garrison,
    /// (from_bell, delta): reinforcements (+) and clash losses (−).
    events: Vec<(u32, i64)>,
}

impl GarrisonTrack {
    fn at(&self, b: u32) -> u32 {
        let v: i64 = self
            .events
            .iter()
            .filter(|(f, _)| *f <= b)
            .map(|(_, d)| d)
            .sum::<i64>();
        (self.garrison.troops as i64 + v).max(0) as u32
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Province {
    entries: Vec<Entry>,
    garrisons: Vec<GarrisonTrack>,
    next_bell: u32,
    outcomes: Vec<[u8; 32]>,
}

/// Postures committed for bell b (fixed by the script, not by state).
fn posture_for(b: u32, id: u64) -> Posture {
    let r = sha256(&[b"posture", &b.to_le_bytes(), &id.to_le_bytes()]);
    posture_of(r[0] % 3 != 0, STANCES.get(r[1] as usize % 5).copied())
}

fn frozen(p: &Province, b: u32) -> (Vec<Fighter>, Vec<Garrison>) {
    let residents = p
        .entries
        .iter()
        .filter(|e| e.presence.in_roster(b))
        .map(|e| Fighter {
            stamina: e.stamina.at(b),
            posture: posture_for(b, e.host.id),
            ..e.host
        })
        .collect();
    let garrisons = p
        .garrisons
        .iter()
        .map(|g| Garrison {
            troops: g.at(b),
            posture: posture_for(b, g.garrison.id),
            ..g.garrison
        })
        .collect();
    (residents, garrisons)
}

fn apply(p: &mut Province, b: u32, o: &ClashOutcome, arrivals: &[Fighter]) {
    for r in &o.fighters {
        let gone = matches!(r.fate, Fate::Bounced | Fate::Retreated | Fate::Destroyed);
        let tile = match r.fate {
            Fate::Stays { tile } | Fate::Withdrew { tile } => Some(tile),
            _ => None,
        };
        if let Some(e) = p.entries.iter_mut().find(|e| e.host.id == r.id) {
            e.host.troops = r.troops;
            e.stamina.set(b, r.stamina);
            if let Some(t) = tile {
                e.host.tile = t;
            }
            if gone {
                e.presence.leave(b);
            }
        } else if let (Some(t), Some(a)) = (tile, arrivals.iter().find(|a| a.id == r.id)) {
            p.entries.push(Entry {
                presence: Presence::from_next(b),
                host: Fighter {
                    troops: r.troops,
                    tile: t,
                    retreat_bps: None,
                    ..*a
                },
                stamina: Stamina {
                    value: r.stamina,
                    bell: b,
                },
            });
        }
    }
    for gr in &o.garrisons {
        let g = p
            .garrisons
            .iter_mut()
            .find(|g| g.garrison.id == gr.id)
            .unwrap();
        let loss = g.at(b) as i64 - gr.troops as i64;
        if loss != 0 {
            g.events.push((b + 1, -loss));
        }
    }
    p.outcomes.push(o.digest());
}

/// What the script does during a bell: resident actions (issued during the
/// bell, effective from the next) and the bell's revealed arrivals.
#[derive(Clone)]
enum Action {
    Muster(Fighter),
    Leave(u64),
    Reinforce(u64, i64),
}

struct Script {
    terrain: ProvinceTerrain,
    initial: Province,
    actions: Vec<Vec<Action>>,
    arrivals: Vec<Vec<Fighter>>,
    relations: Relations,
}

fn script(seed_n: u64, bells: u32) -> Script {
    let mut rng = Rng(seed_n);
    let terrain = generate_province(&seed(b"ring", seed_n % 3), P);
    let hot = [30u8, 31, 38, 22, 23];
    let mut id = 1_000;
    let mut new_host = |rng: &mut Rng, faction: u8| {
        id += 1;
        let mut f = fighter(
            id,
            faction,
            UNITS[rng.below(6) as usize],
            200 + rng.below(6_000) as u32,
            hot[rng.below(5) as usize],
        );
        f.stamina = 40 + rng.below(80) as u16;
        f
    };
    let mut initial = Province {
        entries: Vec::new(),
        garrisons: Vec::new(),
        next_bell: 1,
        outcomes: Vec::new(),
    };
    let mut known: Vec<u64> = Vec::new();
    for _ in 0..8 {
        let fa = rng.below(3) as u8;
        let f = new_host(&mut rng, fa);
        known.push(f.id);
        initial.entries.push(Entry {
            presence: Presence {
                from_bell: 0,
                until_bell: u32::MAX,
            },
            stamina: Stamina {
                value: f.stamina,
                bell: 0,
            },
            host: f,
        });
    }
    for (k, t) in [30u8, 22].iter().enumerate() {
        initial.garrisons.push(GarrisonTrack {
            garrison: Garrison {
                id: 50 + k as u64,
                faction: k as u8,
                tile: *t,
                troops: 1_000 * 1000,
                walls: k == 0,
                posture: Posture::default(),
            },
            events: Vec::new(),
        });
    }
    let mut actions = vec![Vec::new(); bells as usize + 1];
    let mut arrivals = vec![Vec::new(); bells as usize + 1];
    for b in 1..=bells as usize {
        for _ in 0..rng.below(3) {
            match rng.below(3) {
                0 => {
                    let fa = rng.below(3) as u8;
                    let f = new_host(&mut rng, fa);
                    known.push(f.id);
                    actions[b].push(Action::Muster(f));
                }
                1 => actions[b].push(Action::Leave(known[rng.below(known.len() as u64) as usize])),
                _ => actions[b].push(Action::Reinforce(
                    50 + rng.below(2),
                    rng.below(400) as i64 * 1000,
                )),
            }
        }
        for _ in 0..rng.below(5) {
            let fa = rng.below(4) as u8;
            let mut f = new_host(&mut rng, fa);
            f.posture = Posture::Stance(STANCES[rng.below(4) as usize]);
            if rng.chance(25) {
                f.retreat_bps = Some(rng.below(25_000) as u32);
            }
            arrivals[b].push(f);
        }
    }
    let mut relations = Relations::ALL_HOSTILE;
    relations.set_peaceful(1, 2, true);
    Script {
        terrain,
        initial,
        actions,
        arrivals,
        relations,
    }
}

fn issue(p: &mut Province, b: u32, a: &Action) {
    match a {
        Action::Muster(f) => p.entries.push(Entry {
            presence: Presence::from_next(b),
            stamina: Stamina {
                value: f.stamina,
                bell: b,
            },
            host: *f,
        }),
        Action::Leave(id) => {
            if let Some(e) = p.entries.iter_mut().find(|e| e.host.id == *id) {
                e.presence.leave(b);
            }
        }
        Action::Reinforce(g, d) => {
            if let Some(g) = p.garrisons.iter_mut().find(|x| x.garrison.id == *g) {
                g.events.push((b + 1, *d));
            }
        }
    }
}

fn resolve_next(s: &Script, p: &mut Province, rng: &mut Rng) {
    let b = p.next_bell;
    let (mut residents, mut garrisons) = frozen(p, b);
    let mut arrivals = s.arrivals[b as usize].clone();
    // reveals land in any order, accounts are read in any order
    rng.shuffle(&mut residents);
    rng.shuffle(&mut garrisons);
    rng.shuffle(&mut arrivals);
    let o = resolve_clash(
        &frontier_ruleset(),
        &ClashInput {
            province: P,
            bell: b,
            seed: seed(b"bell", b as u64),
            terrain: &s.terrain,
            residents: &residents,
            garrisons: &garrisons,
            arrivals: &arrivals,
            relations: s.relations,
        },
    )
    .unwrap();
    apply(p, b, &o, &s.arrivals[b as usize]);
    p.next_bell = b + 1;
}

/// Play the script with a lag pattern: after the actions of wall bell w,
/// resolve up to `w − 1 − lag(w)` (a bell resolves only after it ended).
fn play(s: &Script, bells: u32, lag: &mut dyn FnMut(u32) -> u32, rng: &mut Rng) -> Province {
    let mut p = s.initial.clone();
    for w in 1..=bells + 60 {
        if w <= bells {
            for a in &s.actions[w as usize] {
                issue(&mut p, w, a);
            }
        }
        let target = (w - 1).saturating_sub(lag(w)).min(bells);
        while p.next_bell <= target {
            resolve_next(s, &mut p, rng);
        }
    }
    while p.next_bell <= bells {
        resolve_next(s, &mut p, rng); // the keepers finally catch up
    }
    p.entries.sort_by_key(|e| e.host.id);
    for g in p.garrisons.iter_mut() {
        g.events.sort(); // the garrison's value is a sum: order is irrelevant
    }
    p
}

/// §9.4 lag invariance: resolving the province's bells with any delay
/// pattern (0 to 50 bells of lag), reveals and accounts in any order,
/// gives identical results.
#[test]
fn clash_results_do_not_depend_on_lag() {
    const BELLS: u32 = 40;
    for n in 0..12u64 {
        let s = script(n, BELLS);
        let mut rng = Rng(n + 1);
        let prompt = play(&s, BELLS, &mut |_| 0, &mut rng);
        assert_eq!(prompt.outcomes.len(), BELLS as usize);
        let mut lag_rng = Rng(n * 31 + 5);
        let random = play(&s, BELLS, &mut |_| lag_rng.below(51) as u32, &mut rng);
        let mut stuck = Rng(n);
        let bursty = play(
            &s,
            BELLS,
            &mut |w| {
                if w % 17 < 12 {
                    50
                } else {
                    stuck.below(3) as u32
                }
            },
            &mut rng,
        );
        let at_end = play(&s, BELLS, &mut |_| 1_000, &mut rng);
        for (name, other) in [
            ("random", &random),
            ("bursty", &bursty),
            ("at end", &at_end),
        ] {
            assert_eq!(prompt.outcomes, other.outcomes, "script {n}: {name}");
            assert_eq!(prompt.entries, other.entries, "script {n}: {name}");
            assert_eq!(prompt.garrisons, other.garrisons, "script {n}: {name}");
        }
    }
}

/// §9.4 roster freeze: for random defender actions issued during bell b
/// (Muster, Depart, Dissolve, garrison changes), the bell-b clash result
/// is identical to the result without them — and applying the same
/// actions at once (no freeze) would change it.
#[test]
fn defender_actions_during_a_bell_do_not_change_its_clash() {
    let mut changed_without_freeze = 0;
    for n in 0..60u64 {
        let s = script(100 + n, 12);
        let mut rng = Rng(n);
        let mut p = s.initial.clone();
        for w in 1..=6 {
            for a in &s.actions[w as usize] {
                issue(&mut p, w, a);
            }
            resolve_next(&s, &mut p, &mut rng);
        }
        let b = p.next_bell; // 7: bell under way, not resolved
        let run_b = |p: &Province| {
            let (r, g) = frozen(p, b);
            clash(&s.terrain, b, &r, &g, &s.arrivals[b as usize], s.relations)
        };
        let base = run_b(&p);
        let mut acted = p.clone();
        let mut ids: Vec<u64> = p.entries.iter().map(|e| e.host.id).collect();
        ids.sort();
        for k in 0..1 + rng.below(6) {
            match rng.below(3) {
                0 => {
                    let mut f =
                        fighter(90_000 + k, rng.below(3) as u8, UnitType::Knight, 9_000, 30);
                    f.stamina = STAMINA_CAP;
                    issue(&mut acted, b, &Action::Muster(f));
                }
                1 => {
                    let id = ids[rng.below(ids.len() as u64) as usize];
                    issue(&mut acted, b, &Action::Leave(id));
                }
                _ => issue(
                    &mut acted,
                    b,
                    &Action::Reinforce(50 + rng.below(2), 5_000_000),
                ),
            }
        }
        assert_eq!(run_b(&acted).digest(), base.digest(), "script {n}");
        // Control: the same actions with no freeze (effective at once).
        let mut unfrozen = acted.clone();
        for e in unfrozen.entries.iter_mut() {
            if e.presence.from_bell == b + 1 {
                e.presence.from_bell = b;
            }
            if e.presence.until_bell == b + 1 {
                e.presence.until_bell = b;
            }
        }
        for g in unfrozen.garrisons.iter_mut() {
            for ev in g.events.iter_mut() {
                if ev.0 == b + 1 && ev.1 > 0 {
                    ev.0 = b;
                }
            }
        }
        changed_without_freeze += (run_b(&unfrozen).digest() != base.digest()) as u32;
    }
    assert!(
        changed_without_freeze > 20,
        "control changed only {changed_without_freeze}"
    );
}

// ------------------------------------------------------------ sieges (§6.3, D9)

const GENESIS: i64 = 1_790_000_000 - 1_790_000_000 % DAY; // a UTC midnight

#[test]
fn sieges_need_six_hours_outside_the_vigil() {
    assert_eq!(required_bells(0, 0), 36);
    assert_eq!(required_bells(149, 0), 38);
    assert_eq!(required_bells(0, 12), 48);
    let vigil = Vigil::new(0).unwrap(); // 00:00–08:00 UTC
                                        // Horn at 23:00: 1 hour before the vigil.
    let horn = bell_at(GENESIS, GENESIS + 23 * HOUR);
    let mut s = Siege::declare(2, horn, 0, 0);
    let hold = BellReport {
        attackers_hold: true,
        defender_present: false,
    };
    let mut b = horn;
    while s.status == SiegeStatus::Active {
        b += 1;
        s.advance(b, bell_start(GENESIS, b), hold, &vigil).unwrap();
    }
    assert_eq!(s.status, SiegeStatus::Completed);
    let done = bell_start(GENESIS, b) + BELL_SECS;
    // The horn bell itself is not counted (it takes effect next bell):
    // 5 bells before the vigil, 8 h of vigil, 31 bells after, so the siege
    // completes at the end of the bell starting 13:00, 14 h 10 min after
    // the horn.
    assert_eq!(done - bell_start(GENESIS, horn), 14 * HOUR + BELL_SECS);
    assert!(
        done >= GENESIS + DAY + 8 * HOUR,
        "not before the defender wakes"
    );
}

#[test]
fn siege_progress_is_counted_once_per_bell_in_order() {
    let vigil = Vigil::new(12 * 3_600).unwrap();
    let mut s = Siege::declare(1, 100, 0, 0);
    let hold = BellReport {
        attackers_hold: true,
        defender_present: false,
    };
    assert!(s
        .advance(102, bell_start(GENESIS, 102), hold, &vigil)
        .is_err());
    s.advance(101, bell_start(GENESIS, 101), hold, &vigil)
        .unwrap();
    assert!(s
        .advance(101, bell_start(GENESIS, 101), hold, &vigil)
        .is_err());
    // a defending host pauses, losing the hex after holding it fails
    let paused = BellReport {
        attackers_hold: true,
        defender_present: true,
    };
    let p = s.progress;
    s.advance(102, bell_start(GENESIS, 102), paused, &vigil)
        .unwrap();
    assert_eq!(s.progress, p);
    let lost = BellReport {
        attackers_hold: false,
        defender_present: true,
    };
    assert_eq!(
        s.advance(103, bell_start(GENESIS, 103), lost, &vigil)
            .unwrap(),
        SiegeStatus::Failed
    );
    // before the besiegers first hold the hex, a siege waits (72 bells)
    let mut w = Siege::declare(1, 0, 0, 0);
    for b in 1..=72 {
        w.advance(b, bell_start(GENESIS, b), lost, &vigil).unwrap();
    }
    assert_eq!(w.status, SiegeStatus::Active);
    w.advance(73, bell_start(GENESIS, 73), lost, &vigil)
        .unwrap();
    assert_eq!(w.status, SiegeStatus::Failed);
}

#[test]
fn vigils_change_weekly_with_notice() {
    let t0 = GENESIS + 3 * DAY;
    let mut v = Vigil::new(0).unwrap();
    assert!(v.covers(t0 + HOUR));
    let from = v.request_change(t0, 10 * 3_600).unwrap();
    assert_eq!(from, t0 + DAY);
    assert!(v.covers(t0 + HOUR), "the old window holds for 24 h");
    assert!(!v.covers(from + HOUR));
    assert!(v.covers(from + 11 * HOUR));
    assert!(matches!(
        v.request_change(t0 + 6 * DAY, 0),
        Err(VigilError::TooSoon { .. })
    ));
    v.request_change(t0 + 7 * DAY, 0).unwrap();
    // a lagging province still judges old bells by the old schedule
    assert!(v.covers(t0 + HOUR));
    assert_eq!(Vigil::new(86_400), Err(VigilError::BadStart));
}

fn check() -> SiegeCheck {
    SiegeCheck {
        province: ProvinceCoord::new(5, 2),
        kind: HoldingKind::Other,
        owner_faction: 0,
        attacker_faction: 3,
        relation: Relation::Rivalry,
        march_hostility: false,
        march_truce: false,
        founded_ts: GENESIS,
        founded_day: 0,
        shield_until: GENESIS + 48 * HOUR,
        dormant: false,
        attacker_nearby: false,
        now: GENESIS + 3 * DAY,
    }
}

#[test]
fn who_may_besiege() {
    assert_eq!(may_besiege(&check()), Ok(()));
    let seat = SiegeCheck {
        province: seat_of(0),
        ..check()
    };
    assert_eq!(may_besiege(&seat), Err(SiegeRefusal::Seat));
    let shielded = SiegeCheck {
        now: GENESIS + HOUR,
        ..check()
    };
    assert_eq!(may_besiege(&shielded), Err(SiegeRefusal::Shielded));
    assert_eq!(
        may_besiege(&SiegeCheck {
            dormant: true,
            ..shielded
        }),
        Ok(()),
        "dormancy lapses the Shield"
    );
    let late = SiegeCheck {
        founded_day: 5,
        now: GENESIS + 4 * DAY,
        ..check()
    };
    assert_eq!(may_besiege(&late), Err(SiegeRefusal::FrontierProtected));
    assert_eq!(
        may_besiege(&SiegeCheck {
            attacker_nearby: true,
            ..late
        }),
        Ok(())
    );
    assert_eq!(
        may_besiege(&SiegeCheck {
            now: GENESIS + 48 * HOUR + 7 * DAY,
            ..late
        }),
        Ok(())
    );
    let heart = SiegeCheck {
        province: ring_provinces(2)
            .into_iter()
            .find(|p| p.wedge() == Some(0))
            .unwrap(),
        ..check()
    };
    assert_eq!(may_besiege(&heart), Err(SiegeRefusal::Heartland));
    assert_eq!(
        may_besiege(&SiegeCheck {
            relation: Relation::War,
            ..heart
        }),
        Ok(())
    );
    assert_eq!(
        may_besiege(&SiegeCheck {
            march_hostility: true,
            ..heart
        }),
        Ok(())
    );
    assert_eq!(
        may_besiege(&SiegeCheck {
            relation: Relation::Nap,
            ..check()
        }),
        Err(SiegeRefusal::Friendly)
    );
    assert_eq!(
        may_besiege(&SiegeCheck {
            march_truce: true,
            ..check()
        }),
        Err(SiegeRefusal::Truce)
    );
    assert_eq!(
        may_besiege(&SiegeCheck {
            kind: HoldingKind::FreeCity,
            ..heart
        }),
        Ok(())
    );
}

#[test]
fn auto_reinforce_stays_in_the_march_and_under_a_quarter() {
    let target = ProvinceCoord::new(4, 1);
    let m = march_members(march_of(target));
    let outside = ProvinceCoord::new(40, -3);
    assert_ne!(march_of(outside), march_of(target));
    let donors = [
        Donor {
            id: 3,
            province: m[2],
            faction: 1,
            garrison: 1_000_000,
            order_bps: 10_000,
        },
        Donor {
            id: 2,
            province: m[0],
            faction: 1,
            garrison: 400_000,
            order_bps: 1_000,
        },
        Donor {
            id: 4,
            province: outside,
            faction: 1,
            garrison: 1_000_000,
            order_bps: 2_500,
        },
        Donor {
            id: 5,
            province: m[1],
            faction: 2,
            garrison: 1_000_000,
            order_bps: 2_500,
        },
        Donor {
            id: 1,
            province: target,
            faction: 1,
            garrison: 1_000_000,
            order_bps: 2_500,
        },
    ];
    assert_eq!(
        auto_reinforce(1, target, 1, &donors),
        vec![(2, 40_000), (3, 250_000)]
    );
}

#[test]
fn raids_take_a_tenth_at_most_every_six_hours() {
    let mut h = Holding::found(0, 0, 2);
    h.credit(0, Resource::Gold, 1_000 * MILLI).unwrap();
    let loot = raid(&mut h, None, HOUR).unwrap();
    assert_eq!(loot[Resource::Gold as usize], 100 * MILLI);
    assert_eq!(h.stock_at(HOUR)[Resource::Gold as usize], 900 * MILLI);
    assert!(raid(&mut h, Some(HOUR), 6 * HOUR).is_none());
    assert!(raid(&mut h, Some(HOUR), 7 * HOUR).is_some());
}

// ------------------------------------------------------------ travel (§3.6, D10)

#[test]
fn travel_times_match_the_design_table() {
    assert_eq!(
        hex_secs(Terrain::Plains, false, UnitType::Spearman),
        Some(120)
    );
    assert_eq!(
        hex_secs(Terrain::Hills, false, UnitType::Spearman),
        Some(180)
    );
    assert_eq!(
        hex_secs(Terrain::Forest, true, UnitType::Spearman),
        Some(90)
    );
    assert_eq!(hex_secs(Terrain::Plains, false, UnitType::Knight), Some(60));
    assert_eq!(
        hex_secs(Terrain::Plains, true, UnitType::Horseman),
        Some(30)
    );
    assert_eq!(hex_secs(Terrain::Mountain, true, UnitType::Knight), None);
    assert_eq!(hex_secs(Terrain::Water, false, UnitType::Scout), None);
    // Rim to Concord (design §3.6): 55 hexes 1.8 h on foot, 0.9 h by road
    // or horse; 548 hexes 18 h.
    assert_eq!(open_ground_secs(55, false, UnitType::Spearman), 110 * 60);
    assert_eq!(open_ground_secs(55, true, UnitType::Spearman), 55 * 60);
    assert_eq!(open_ground_secs(55, false, UnitType::Knight), 55 * 60);
    assert_eq!(open_ground_secs(548, false, UnitType::Spearman) / 60, 1_096); // 18.3 h
}

#[test]
fn paths_are_checked_not_searched() {
    let start = Hex::new(0, 0);
    let step = |q, r, terrain| Step {
        hex: Hex::new(q, r),
        terrain,
        road: false,
    };
    let ok = [
        step(1, 0, Terrain::Plains),
        step(2, 0, Terrain::Hills),
        step(2, 1, Terrain::Plains),
    ];
    let c = path_cost(start, &ok, UnitType::Spearman).unwrap();
    assert_eq!((c.secs, c.hexes), (420, 3));
    assert_eq!(
        path_cost(start, &[step(2, 0, Terrain::Plains)], UnitType::Spearman),
        Err(TravelError::NotAdjacent(0))
    );
    assert_eq!(
        path_cost(start, &[step(1, 0, Terrain::Water)], UnitType::Spearman),
        Err(TravelError::Impassable(0))
    );
    assert_eq!(
        path_cost(start, &[], UnitType::Spearman),
        Err(TravelError::EmptyPath)
    );
    let long: Vec<Step> = (1..=33).map(|q| step(q, 0, Terrain::Plains)).collect();
    assert_eq!(
        path_cost(start, &long, UnitType::Spearman),
        Err(TravelError::TooLong)
    );
    // A path may enter at most 4 provinces (the accounts a Reveal reads):
    // straight 32-hex lines in all six directions, accepted exactly when
    // they touch ≤ 4.
    let mut refused = 0;
    for (dq, dr) in permutation_rules::hex::DIRECTIONS {
        let line: Vec<Step> = (1..=32)
            .map(|i| step(dq * i, dr * i, Terrain::Plains))
            .collect();
        let mut touched: Vec<ProvinceCoord> = line.iter().map(|s| province_of(s.hex)).collect();
        touched.sort();
        touched.dedup();
        match path_cost(start, &line, UnitType::Spearman) {
            Ok(c) => {
                assert!(touched.len() <= MAX_PATH_PROVINCES);
                assert_eq!(c.provinces.len(), touched.len());
                assert_eq!(c.secs, 32 * 120);
            }
            Err(e) => {
                assert_eq!(e, TravelError::TooManyProvinces);
                assert!(touched.len() > MAX_PATH_PROVINCES);
                refused += 1;
            }
        }
    }
    // Measured: every straight 32-hex line touches 5 or 6 provinces, so the
    // 4-province bound binds first (straight marches of 20–28 hexes).
    assert_eq!(refused, 6);
    for start in hexes_within(4) {
        for (dq, dr) in permutation_rules::hex::DIRECTIONS {
            let line: Vec<Step> = (1..=20)
                .map(|i| step(start.q + dq * i, start.r + dr * i, Terrain::Plains))
                .collect();
            assert!(path_cost(start, &line, UnitType::Spearman).is_ok());
        }
    }
    assert!(path_cost(Hex::new(COORD_BOUND + 1, 0), &ok, UnitType::Spearman).is_err());
}

#[test]
fn arrivals_round_up_to_the_next_bell() {
    let d = GENESIS + 10 * BELL_SECS + 30; // 30 s into bell 10
    assert_eq!(earliest_arrival_bell(GENESIS, d, 60), 11);
    assert_eq!(earliest_arrival_bell(GENESIS, d, 570), 11); // lands exactly at bell 11
    assert_eq!(earliest_arrival_bell(GENESIS, d, 571), 12);
    assert_eq!(
        earliest_arrival_bell(GENESIS, d, 0),
        11,
        "never the departure bell"
    );
    assert!(check_arrival_bell(GENESIS, d, 571, 12).is_ok());
    assert_eq!(
        check_arrival_bell(GENESIS, d, 571, 11),
        Err(TravelError::TooEarly { earliest: 12 })
    );
    assert!(check_arrival_bell(GENESIS, d, 571, 82).is_ok());
    assert_eq!(
        check_arrival_bell(GENESIS, d, 571, 83),
        Err(TravelError::TooLate { latest: 82 })
    );
    assert_eq!(march_stamina(32), 74);
    assert!(march_stamina(32) < STAMINA_CAP);
    assert!(out_of_supply(
        ProvinceCoord::new(9, 0),
        &[ProvinceCoord::new(5, 0)]
    ));
    assert!(!out_of_supply(
        ProvinceCoord::new(8, 0),
        &[ProvinceCoord::new(5, 0)]
    ));
}
