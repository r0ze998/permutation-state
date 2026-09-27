//! Frontier (rules v10) world kernels: geometry, terrain, holdings, hosts,
//! stances, the clash, sieges and travel (design §3, §5.1, §6), and the
//! §9.4 property tests that concern them: determinism and order
//! independence of the clash, the roster freeze, lag invariance and quota
//! fairness (the ArrivalSlot set), plus the seed-round margin and the
//! doctrine table's bounds.

use permutation_rules::fixed::{BPS_ONE, MILLI};
use permutation_rules::frontier::clash::Occupancy;
use permutation_rules::frontier::clash::{
    admit_arrival, apply_slot, clash_seed, frontier_ruleset, is_quiet, quota_set, resolve_clash,
    reveal_close, reveal_open, seed_round, BeaconClock, ClashInput, ClashOutcome, FactionSlots,
    Fate, Fighter, Garrison, Relations, RelationsLog, SlotDecision, SlotEntry, SlotRefusal,
    FACTION_ARRIVAL_SLOTS, QUICKNET, REVEAL_WINDOW_SECS, SEED_MARGIN_SECS,
};
use permutation_rules::frontier::doctrine::{
    self, validate_table, Doctrine, DoctrineError, DOCTRINES,
};
use permutation_rules::frontier::geometry::*;
use permutation_rules::frontier::holding::{
    duplicate_cost, shield_secs, Accrual, Effect, Holding, Resource, Tier, DAY, HOUR,
};
use permutation_rules::frontier::host::{
    rout_survivors, settle_merge, supply_attrition, GarrisonState, Host, HostError, PendingOp,
    Presence, Stamina, DESTROYED_BELOW, ENGAGE_STAMINA, MAX_HOST_TROOPS, MIN_HOST_TROOPS,
    STAMINA_CAP,
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
            occupancy: Occupancy::EMPTY,
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
    let mut b = Host::muster(2, 9, 0, UnitType::Knight, 10_001 * 1000, 10).unwrap();
    assert_eq!(a.merge(&mut b, 10, 10), Err(HostError::TooLarge));
    // A change needs its province resolved through bell b − 2.
    assert_eq!(a.split(5_000 * 1000, 3, 11, 9), Err(HostError::Unresolved));
    // A split issued during bell 11 takes effect after the clash of 11,
    // which still sees the whole host.
    a.split(5_000 * 1000, 3, 11, 11).unwrap();
    assert_eq!(a.values_at(11).unwrap().0, 20_000 * 1000);
    assert_eq!(a.split(200_000, 4, 11, 11), Err(HostError::Busy));
    assert_eq!(a.values_at(12), Err(HostError::Unsettled));
    assert_eq!(a.settle(11).unwrap(), None, "bell 11 not resolved yet");
    // The clash of 11 costs the host 10%: the split-off host gets its
    // share of the survivors.
    a.apply_clash(11, 18_000 * 1000, STAMINA_CAP, true).unwrap();
    let mut c = a.settle(12).unwrap().unwrap();
    assert_eq!((a.troops, c.troops), (13_500 * 1000, 4_500 * 1000));
    assert_eq!(c.id, 3);
    // A Depart during bell 13 pays its march after the clash of 13.
    assert_eq!(a.depart(12, 10, 12), Err(HostError::Cooldown));
    a.depart(13, 100, 13).unwrap();
    assert_eq!(a.values_at(13).unwrap().1, STAMINA_CAP);
    assert_eq!(a.march_values(13, 13), Err(HostError::Unresolved));
    a.settle(14).unwrap();
    assert_eq!(a.march_values(13, 14).unwrap(), (13_500 * 1000, 20));
    // Merging: both fight separately in the merge bell, then join.
    let mut a = Host::muster(1, 9, 0, UnitType::Knight, 10_000 * 1000, 10).unwrap();
    a.stamina.set(10, 20);
    a.merge(&mut c, 15, 15).unwrap();
    assert_eq!(a.values_at(15).unwrap().0, 10_000 * 1000);
    assert_eq!(settle_merge(&mut a, &mut c, 15), Err(HostError::Unresolved));
    settle_merge(&mut a, &mut c, 16).unwrap();
    assert_eq!((a.troops, c.troops), (14_500 * 1000, 0));
    assert_eq!(a.stamina.at(16), 26, "the lower stamina wins");
    a.route(16);
    assert_eq!(a.troops, rout_survivors(14_500 * 1000));
    assert_eq!(a.stamina.at(16), 0);
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

/// A resident of the test province: its roster presence, the kernel host
/// and its tile.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    presence: Presence,
    host: Host,
    tile: u8,
}

/// A holding of the test province: garrison state and the holding (walls).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Site {
    id: u64,
    faction: u8,
    tile: u8,
    garrison: GarrisonState,
    holding: Holding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Province {
    entries: Vec<Entry>,
    sites: Vec<Site>,
    relations: RelationsLog,
    /// First bell not yet resolved.
    next_bell: u32,
    outcomes: Vec<[u8; 32]>,
}

/// Postures committed for bell b (fixed by the script, not by state).
fn posture_for(b: u32, id: u64) -> Posture {
    let r = sha256(&[b"posture", &b.to_le_bytes(), &id.to_le_bytes()]);
    posture_of(r[0] % 3 != 0, STANCES.get(r[1] as usize % 5).copied())
}

/// The clash inputs of bell b, all read as of the start of b.
fn frozen(p: &Province, b: u32) -> (Vec<Fighter>, Vec<Garrison>, Relations) {
    let residents = p
        .entries
        .iter()
        .filter(|e| e.presence.in_roster(b))
        .map(|e| {
            let (troops, stamina) = e.host.values_at(b).expect("settled");
            Fighter {
                id: e.host.id,
                faction: e.host.faction,
                unit: e.host.unit,
                troops,
                stamina,
                tile: e.tile,
                posture: posture_for(b, e.host.id),
                retreat_bps: None,
                dealt_bps: BPS_ONE,
            }
        })
        .collect();
    let garrisons = p
        .sites
        .iter()
        .map(|g| Garrison {
            id: g.id,
            faction: g.faction,
            tile: g.tile,
            troops: g.garrison.at(b).expect("settled"),
            walls: g
                .holding
                .walls_at(bell_start(GENESIS, b))
                .expect("committed")
                > 0,
            posture: posture_for(b, g.id),
        })
        .collect();
    (residents, garrisons, p.relations.at(b))
}

/// Execute every change whose bell's clash has been applied.
fn settle_all(p: &mut Province) {
    let rn = p.next_bell;
    let mut born = Vec::new();
    for k in 0..p.entries.len() {
        match p.entries[k].host.pending {
            Some(x) if x.bell < rn => match x.op {
                PendingOp::Absorb { from } => {
                    let j = p.entries.iter().position(|e| e.host.id == from).unwrap();
                    let mut a = p.entries[k].host;
                    let mut c = p.entries[j].host;
                    settle_merge(&mut a, &mut c, rn).unwrap();
                    p.entries[k].host = a;
                    p.entries[j].host = c;
                }
                PendingOp::AbsorbedInto { .. } => {}
                _ => {
                    let child = p.entries[k].host.settle(rn).unwrap();
                    let e = &p.entries[k];
                    if let Some(c) = child {
                        if e.presence.in_roster(x.bell + 1) && c.troops >= DESTROYED_BELOW {
                            born.push(Entry {
                                presence: Presence::from_next(x.bell),
                                host: c,
                                tile: e.tile,
                            });
                        }
                    }
                }
            },
            _ => {}
        }
    }
    p.entries.extend(born);
    for g in p.sites.iter_mut() {
        g.garrison.settle(rn);
        g.holding.commit_walls(bell_start(GENESIS, rn));
    }
}

fn apply(p: &mut Province, b: u32, o: &ClashOutcome, arrivals: &[Fighter]) {
    for r in &o.fighters {
        let gone = matches!(r.fate, Fate::Bounced | Fate::Retreated | Fate::Destroyed);
        let tile = match r.fate {
            Fate::Stays { tile } | Fate::Withdrew { tile } => Some(tile),
            _ => None,
        };
        if let Some(e) = p.entries.iter_mut().find(|e| e.host.id == r.id) {
            e.host
                .apply_clash(b, r.troops, r.stamina, r.engaged)
                .unwrap();
            if let Some(t) = tile {
                e.tile = t;
            }
            if gone {
                e.presence.leave(b);
            }
        } else if let (Some(t), Some(a)) = (tile, arrivals.iter().find(|a| a.id == r.id)) {
            let mut host = Host::muster(
                a.id,
                a.faction as u64,
                a.faction,
                a.unit,
                MIN_HOST_TROOPS,
                b,
            )
            .unwrap();
            host.apply_clash(b, r.troops, r.stamina, r.engaged).unwrap();
            p.entries.push(Entry {
                presence: Presence::from_next(b),
                host,
                tile: t,
            });
        }
    }
    for gr in &o.garrisons {
        let g = p.sites.iter_mut().find(|g| g.id == gr.id).unwrap();
        g.garrison.apply_clash(b, gr.troops).unwrap();
    }
    p.outcomes.push(o.digest());
    p.next_bell = b + 1;
    settle_all(p);
}

/// What the script does during a bell: resident actions (issued during the
/// bell, effective from the next), wall completions and decrees.
#[derive(Clone, Debug)]
enum Action {
    Muster(Fighter),
    /// Depart: leaves the roster and pays its march after the clash.
    Depart(u64),
    Split(u64, u64),
    Merge(u64, u64),
    Reinforce(u64, i64),
    /// Walls on a site, finishing `secs` after the order (mid-bell).
    Walls(u64, i64),
    Decree(u8, u8, bool),
}

impl Action {
    /// Resident actions write the province: the issuer first resolves it
    /// through b − 2 (the client bundles `ResolveClash`, §8.4).
    fn resident(&self) -> bool {
        !matches!(self, Action::Decree(..))
    }
}

struct Script {
    terrain: ProvinceTerrain,
    initial: Province,
    actions: Vec<Vec<Action>>,
    arrivals: Vec<Vec<Fighter>>,
}

/// Units of the scripted hosts: few, so that merges find partners.
const SCRIPT_UNITS: [UnitType; 3] = [UnitType::Spearman, UnitType::Knight, UnitType::Archer];

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
            SCRIPT_UNITS[rng.below(3) as usize],
            200 + rng.below(6_000) as u32,
            hot[rng.below(5) as usize],
        );
        f.stamina = 40 + rng.below(80) as u16;
        f
    };
    let host_of = |f: &Fighter| {
        let mut h = Host::muster(
            f.id,
            f.faction as u64,
            f.faction,
            f.unit,
            MIN_HOST_TROOPS,
            0,
        )
        .unwrap();
        h.troops = f.troops;
        h.stamina = Stamina {
            value: f.stamina,
            bell: 0,
        };
        h
    };
    let mut initial = Province {
        entries: Vec::new(),
        sites: Vec::new(),
        relations: {
            let mut r = Relations::ALL_HOSTILE;
            r.set_peaceful(1, 2, true);
            RelationsLog::new(r)
        },
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
            host: host_of(&f),
            tile: f.tile,
        });
    }
    for (k, t) in [30u8, 22].iter().enumerate() {
        let mut holding = Holding::found(bell_start(GENESIS, 0), 0, 1);
        holding.tier = Tier::City; // four queue slots
        initial.sites.push(Site {
            id: 50 + k as u64,
            faction: k as u8,
            tile: *t,
            garrison: GarrisonState::new(1_000 * 1000),
            holding,
        });
    }
    let mut actions = vec![Vec::new(); bells as usize + 1];
    let mut arrivals = vec![Vec::new(); bells as usize + 1];
    let mut next_id = 50_000;
    for b in 1..=bells as usize {
        for _ in 0..rng.below(4) {
            let pick =
                |rng: &mut Rng, known: &Vec<u64>| known[rng.below(known.len() as u64) as usize];
            match rng.below(7) {
                0 => {
                    let fa = rng.below(3) as u8;
                    let f = new_host(&mut rng, fa);
                    known.push(f.id);
                    actions[b].push(Action::Muster(f));
                }
                1 => actions[b].push(Action::Depart(pick(&mut rng, &known))),
                2 => {
                    next_id += 1;
                    let parent = pick(&mut rng, &known);
                    known.push(next_id);
                    actions[b].push(Action::Split(parent, next_id));
                }
                3 => {
                    let (x, y) = (pick(&mut rng, &known), pick(&mut rng, &known));
                    actions[b].push(Action::Merge(x, y));
                }
                4 => actions[b].push(Action::Reinforce(
                    50 + rng.below(2),
                    rng.below(600) as i64 * 1000 - 200_000,
                )),
                5 => actions[b].push(Action::Walls(
                    50 + rng.below(2),
                    60 + rng.below(1_500) as i64,
                )),
                _ => actions[b].push(Action::Decree(
                    rng.below(4) as u8,
                    rng.below(4) as u8,
                    rng.chance(50),
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
    Script {
        terrain,
        initial,
        actions,
        arrivals,
    }
}

/// Issue one action during bell b. Refusals (cooldown, busy host, full
/// queue …) depend only on state settled before b, so they are part of the
/// script's deterministic result.
fn issue(p: &mut Province, b: u32, a: &Action) {
    let rn = p.next_bell;
    let present = |p: &Province, id: u64| {
        p.entries.iter().position(|e| {
            e.host.id == id && e.presence.in_roster(b) && e.presence.until_bell == u32::MAX
        })
    };
    match a {
        Action::Muster(f) => {
            let mut h = Host::muster(
                f.id,
                f.faction as u64,
                f.faction,
                f.unit,
                MIN_HOST_TROOPS,
                b,
            )
            .unwrap();
            h.troops = f.troops;
            h.stamina = Stamina {
                value: f.stamina,
                bell: b,
            };
            p.entries.push(Entry {
                presence: Presence::from_next(b),
                host: h,
                tile: f.tile,
            });
        }
        Action::Depart(id) => {
            if let Some(k) = present(p, *id) {
                if p.entries[k].host.depart(b, 10, rn).is_ok() {
                    p.entries[k].presence.leave(b);
                }
            }
        }
        Action::Split(id, new_id) => {
            if let Some(k) = present(p, *id) {
                let t = p.entries[k].host.troops / 3;
                let _ = p.entries[k].host.split(t, *new_id, b, rn);
            }
        }
        Action::Merge(x, y) => {
            if let (Some(i), Some(j)) = (present(p, *x), present(p, *y)) {
                if i != j && p.entries[i].tile == p.entries[j].tile {
                    let mut a = p.entries[i].host;
                    let mut c = p.entries[j].host;
                    if a.merge(&mut c, b, rn).is_ok() {
                        p.entries[i].host = a;
                        p.entries[j].host = c;
                        p.entries[j].presence.leave(b);
                    }
                }
            }
        }
        Action::Reinforce(g, d) => {
            if let Some(g) = p.sites.iter_mut().find(|x| x.id == *g) {
                let _ = g.garrison.change(b, *d, rn);
            }
        }
        Action::Walls(g, secs) => {
            if let Some(g) = p.sites.iter_mut().find(|x| x.id == *g) {
                let now = bell_start(GENESIS, b) + 30;
                let _ = g.holding.enqueue(now, *secs, Effect::Walls { delta: 60 });
            }
        }
        Action::Decree(x, y, peaceful) => {
            p.relations.decree(b, *x, *y, *peaceful);
        }
    }
}

fn resolve_next(s: &Script, p: &mut Province, rng: &mut Rng) {
    let b = p.next_bell;
    let (mut residents, mut garrisons, relations) = frozen(p, b);
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
            relations,
            occupancy: Occupancy::EMPTY,
        },
    )
    .unwrap();
    apply(p, b, &o, &s.arrivals[b as usize]);
}

/// Play the script with a lag pattern: during wall bell w, resolve up to
/// `w − 2 − lag(w)` (a bell resolves at the earliest one bell after it
/// ended); a resident action first resolves through `w − 2`.
fn play(s: &Script, bells: u32, lag: &mut dyn FnMut(u32) -> u32, rng: &mut Rng) -> Province {
    let mut p = s.initial.clone();
    for w in 1..=bells + 60 {
        if w <= bells {
            let acts = &s.actions[w as usize];
            if acts.iter().any(Action::resident) {
                while p.next_bell + 2 <= w {
                    resolve_next(s, &mut p, rng);
                }
            }
            for a in acts {
                issue(&mut p, w, a);
            }
        }
        let target = w.saturating_sub(2 + lag(w)).min(bells);
        while p.next_bell <= target {
            resolve_next(s, &mut p, rng);
        }
    }
    while p.next_bell <= bells {
        resolve_next(s, &mut p, rng); // the keepers finally catch up
    }
    p.entries.sort_by_key(|e| e.host.id);
    p
}

/// §9.4 lag invariance: resolving the province's bells with any delay
/// pattern (0 to 50 bells of lag), reveals and accounts in any order,
/// gives identical results — with merges, splits, Departs, garrison
/// changes, wall completions and decrees landing in the middle of bells.
#[test]
fn clash_results_do_not_depend_on_lag() {
    const BELLS: u32 = 40;
    let mut kinds = [0u32; 7];
    for n in 0..12u64 {
        let s = script(n, BELLS);
        for a in s.actions.iter().flatten() {
            kinds[match a {
                Action::Muster(_) => 0,
                Action::Depart(_) => 1,
                Action::Split(..) => 2,
                Action::Merge(..) => 3,
                Action::Reinforce(..) => 4,
                Action::Walls(..) => 5,
                Action::Decree(..) => 6,
            }] += 1;
        }
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
            assert_eq!(prompt.sites, other.sites, "script {n}: {name}");
        }
    }
    assert!(kinds.iter().all(|k| *k > 10), "action mix {kinds:?}");
}

/// §9.4 roster freeze: for random actions issued during bell b (Muster,
/// Depart with its march stamina, Dissolve, merges, splits, garrison
/// changes, wall completions, decrees), the bell-b clash result is
/// identical to the result without them — and applying the same actions
/// at once (no freeze) would change it.
#[test]
fn actions_during_a_bell_do_not_change_its_clash() {
    let mut changed_without_freeze = 0;
    let mut kinds = [0u32; 7];
    for n in 0..80u64 {
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
            let (r, g, rel) = frozen(p, b);
            clash(&s.terrain, b, &r, &g, &s.arrivals[b as usize], rel)
        };
        let base = run_b(&p);
        let mut acted = p.clone();
        let mut ids: Vec<u64> = p
            .entries
            .iter()
            .filter(|e| e.presence.in_roster(b))
            .map(|e| e.host.id)
            .collect();
        ids.sort();
        if ids.is_empty() {
            continue;
        }
        let pick = |rng: &mut Rng| ids[rng.below(ids.len() as u64) as usize];
        for k in 0..1 + rng.below(6) {
            let kind = rng.below(7);
            kinds[kind as usize] += 1;
            let a = match kind {
                0 => {
                    let mut f =
                        fighter(90_000 + k, rng.below(3) as u8, UnitType::Knight, 9_000, 30);
                    f.stamina = STAMINA_CAP;
                    Action::Muster(f)
                }
                1 => Action::Depart(pick(&mut rng)),
                2 => Action::Split(pick(&mut rng), 80_000 + k),
                3 => Action::Merge(pick(&mut rng), pick(&mut rng)),
                4 => Action::Reinforce(50 + rng.below(2), 5_000_000),
                5 => Action::Walls(50 + rng.below(2), 60),
                _ => Action::Decree(rng.below(3) as u8, rng.below(3) as u8, rng.chance(50)),
            };
            issue(&mut acted, b, &a);
        }
        assert_eq!(run_b(&acted).digest(), base.digest(), "script {n}");
        // Control: the same actions with no freeze (effective at once).
        let mut unfrozen = acted.clone();
        unfrozen.next_bell = b + 1;
        settle_all(&mut unfrozen); // pending changes executed before the clash
        for e in unfrozen.entries.iter_mut() {
            if e.presence.from_bell == b + 1 {
                e.presence.from_bell = b;
            }
            if e.presence.until_bell == b + 1 {
                e.presence.until_bell = b;
            }
        }
        let (r, g, _) = frozen(&unfrozen, b + 1);
        let r: Vec<Fighter> = r
            .into_iter()
            .map(|f| Fighter {
                posture: posture_for(b, f.id),
                ..f
            })
            .collect();
        let g: Vec<Garrison> = g
            .into_iter()
            .map(|x| Garrison {
                posture: posture_for(b, x.id),
                ..x
            })
            .collect();
        let rel = unfrozen.relations.at(b + 1);
        let now = clash(&s.terrain, b, &r, &g, &s.arrivals[b as usize], rel);
        changed_without_freeze += (now.digest() != base.digest()) as u32;
    }
    assert!(kinds.iter().all(|k| *k > 5), "action mix {kinds:?}");
    assert!(
        changed_without_freeze > 30,
        "control changed only {changed_without_freeze}"
    );
}

/// A two-province check of the departure rule: a host departing P during
/// bell d fights in P's clash of d and arrives at Q no earlier than d + 2
/// with the troops and stamina P's clash left it (the arrival waits for
/// P's resolution, lag only waits). Q's results do not depend on how late
/// either province is resolved, and a host destroyed at P never arrives.
#[test]
fn arrivals_carry_the_origin_clash_whatever_the_lag() {
    const BELLS: u32 = 30;
    const Q: ProvinceCoord = ProvinceCoord { p: 4, q: 1 };
    struct March {
        host: u64,
        depart: u32,
        arrive: u32,
        tile: u8,
    }
    #[derive(Clone)]
    struct World {
        p: Province,
        q: Province,
        /// Arrivals of Q by bell: (march index).
        voided: u32,
    }
    for n in 0..8u64 {
        let s = script(500 + n, BELLS);
        let mut rng = Rng(900 + n);
        // Departures from P: a march per bell or so, 2..5 bells long.
        let mut marches: Vec<(u32, u64, u32, u8)> = Vec::new(); // (depart, pick, lead, tile)
        for d in 1..=BELLS - 6 {
            if rng.chance(70) {
                marches.push((
                    d,
                    rng.next(),
                    2 + rng.below(4) as u32,
                    [30u8, 22, 38][rng.below(3) as usize],
                ));
            }
        }
        let q_terrain = generate_province(&seed(b"ring", 7), Q);
        let q_arrivals = |w: &World, b: u32, plan: &[March]| -> Option<Vec<Fighter>> {
            let mut v = Vec::new();
            for m in plan.iter().filter(|m| m.arrive == b) {
                let e = w.p.entries.iter().find(|e| e.host.id == m.host)?;
                let (troops, stamina) = e.host.march_values(m.depart, w.p.next_bell).ok()?;
                if troops < DESTROYED_BELOW {
                    continue; // destroyed at the origin: nothing arrives
                }
                v.push(Fighter {
                    id: m.host,
                    faction: e.host.faction,
                    unit: e.host.unit,
                    troops,
                    stamina,
                    tile: m.tile,
                    posture: Posture::Stance(STANCES[(m.host % 4) as usize]),
                    retreat_bps: None,
                    dealt_bps: BPS_ONE,
                });
            }
            Some(v)
        };
        let run = |lag_p: &mut dyn FnMut(u32) -> u32, lag_q: &mut dyn FnMut(u32) -> u32| {
            let mut rng = Rng(n);
            let mut q = s.initial.clone();
            for e in q.entries.iter_mut() {
                e.host.id += 1_000_000; // Q's own residents
            }
            let mut w = World {
                p: s.initial.clone(),
                q,
                voided: 0,
            };
            let mut plan: Vec<March> = Vec::new();
            let resolve_p = |w: &mut World, rng: &mut Rng| resolve_next(&s, &mut w.p, rng);
            let resolve_q = |w: &mut World, plan: &[March]| -> bool {
                let b = w.q.next_bell;
                let Some(arr) = q_arrivals(w, b, plan) else {
                    return false; // waits for P
                };
                w.voided += plan.iter().filter(|m| m.arrive == b).count() as u32 - arr.len() as u32;
                let (r, g, rel) = frozen(&w.q, b);
                let o = resolve_clash(
                    &frontier_ruleset(),
                    &ClashInput {
                        province: Q,
                        bell: b,
                        seed: seed(b"bell-q", b as u64),
                        terrain: &q_terrain,
                        residents: &r,
                        garrisons: &g,
                        arrivals: &arr,
                        relations: rel,
                        occupancy: Occupancy::EMPTY,
                    },
                )
                .unwrap();
                apply(&mut w.q, b, &o, &arr);
                true
            };
            for t in 1..=BELLS + 80 {
                if t <= BELLS {
                    for &(d, pick, lead, tile) in marches.iter().filter(|m| m.0 == t) {
                        while w.p.next_bell + 2 <= t {
                            resolve_p(&mut w, &mut rng);
                        }
                        let ids: Vec<u64> =
                            w.p.entries
                                .iter()
                                .filter(|e| {
                                    e.presence.in_roster(d) && e.presence.until_bell == u32::MAX
                                })
                                .map(|e| e.host.id)
                                .collect();
                        if ids.is_empty() {
                            continue;
                        }
                        let id = ids[(pick % ids.len() as u64) as usize];
                        let k = w.p.entries.iter().position(|e| e.host.id == id).unwrap();
                        let rn = w.p.next_bell;
                        if w.p.entries[k].host.depart(d, 10, rn).is_ok() {
                            w.p.entries[k].presence.leave(d);
                            assert!(d + lead >= d + MIN_ARRIVAL_LEAD_BELLS);
                            plan.push(March {
                                host: id,
                                depart: d,
                                arrive: d + lead,
                                tile,
                            });
                        }
                    }
                }
                let tp = t.saturating_sub(2 + lag_p(t)).min(BELLS);
                while w.p.next_bell <= tp {
                    resolve_p(&mut w, &mut rng);
                }
                let tq = t.saturating_sub(2 + lag_q(t)).min(BELLS);
                while w.q.next_bell <= tq && resolve_q(&mut w, &plan) {}
            }
            while w.p.next_bell <= BELLS {
                resolve_p(&mut w, &mut rng);
            }
            while w.q.next_bell <= BELLS {
                assert!(resolve_q(&mut w, &plan));
            }
            w.q.entries.sort_by_key(|e| e.host.id);
            (
                w.q.outcomes.clone(),
                w.q.entries.clone(),
                w.voided,
                plan.len(),
            )
        };
        let prompt = run(&mut |_| 0, &mut |_| 0);
        assert!(prompt.3 > 5, "script {n}: only {} marches", prompt.3);
        let mut a = Rng(n * 7 + 1);
        let mut b = Rng(n * 7 + 2);
        let lagged = run(&mut |_| a.below(20) as u32, &mut |_| b.below(3) as u32);
        let mut c = Rng(n * 7 + 3);
        let q_first = run(&mut |_| 25, &mut |_| c.below(2) as u32);
        assert_eq!(prompt, lagged, "script {n}: lagged origin");
        assert_eq!(prompt, q_first, "script {n}: destination keeper ahead");
    }
}

/// Relations, walls and garrisons are read as of the bell's start: a
/// decree or a wall completion in the middle of bell b counts from b + 1
/// (War after its 36-bell horn), however late the clash of b resolves.
#[test]
fn decrees_and_walls_count_from_the_next_bell() {
    let mut log = RelationsLog::new(Relations::ALL_HOSTILE);
    assert_eq!(log.decree(10, 0, 1, true), 11);
    assert!(log.at(10).hostile(0, 1));
    assert!(!log.at(11).hostile(0, 1));
    assert_eq!(log.decree(12, 1, 0, false), 48, "War after its horn");
    assert!(!log.at(47).hostile(0, 1));
    assert!(log.at(48).hostile(0, 1));
    log.prune(20);
    assert!(!log.at(20).hostile(0, 1) && log.at(48).hostile(0, 1));

    let t0 = bell_start(GENESIS, 0);
    let mut h = Holding::found(t0, 0, 1);
    let done = h
        .enqueue(
            bell_start(GENESIS, 5) + 100,
            200,
            Effect::Walls { delta: 60 },
        )
        .unwrap();
    assert_eq!(bell_at(GENESIS, done), 5);
    // A keeper settles the holding long after; the clash of 5 still sees
    // no walls and the clash of 6 sees them.
    h.settle(bell_start(GENESIS, 40)).unwrap();
    assert_eq!(h.walls_at(bell_start(GENESIS, 5)), Ok(0));
    assert_eq!(h.walls_at(bell_start(GENESIS, 6)), Ok(60));
    h.commit_walls(bell_start(GENESIS, 6));
    assert_eq!(h.walls, 60);
    assert!(
        h.walls_at(bell_start(GENESIS, 5)).is_err(),
        "already committed"
    );
    assert_eq!(h.walls_at(bell_start(GENESIS, 6)), Ok(60));

    let mut g = GarrisonState::new(1_000);
    g.change(7, 500, 6).unwrap();
    assert_eq!(g.change(7, 1, 5), Err(HostError::Unresolved));
    g.change(8, -200, 7).unwrap();
    assert_eq!(g.at(7), Ok(1_000));
    assert_eq!(g.at(8), Err(HostError::Unsettled));
    g.apply_clash(7, 900).unwrap(); // the clash of 7 cost 100
    g.settle(8);
    assert_eq!(g.at(8), Ok(1_400));
    g.apply_clash(8, 1_400).unwrap();
    g.settle(9);
    assert_eq!(g.at(9), Ok(1_200));
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
        holders: 1 << 2,
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
        holders: 1 << 1,
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
        holders: 1 << 1,
        defender_present: true,
    };
    let p = s.progress;
    s.advance(102, bell_start(GENESIS, 102), paused, &vigil)
        .unwrap();
    assert_eq!(s.progress, p);
    let lost = BellReport {
        holders: 0,
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
                                           // A host that departs during bell 10 still fights in its origin's
                                           // clash of bell 10, so it cannot arrive before bell 12.
    assert_eq!(earliest_arrival_bell(GENESIS, d, 60), 12);
    assert_eq!(earliest_arrival_bell(GENESIS, d, 1_170), 12); // lands exactly at bell 12
    assert_eq!(earliest_arrival_bell(GENESIS, d, 1_171), 13);
    assert_eq!(
        earliest_arrival_bell(GENESIS, d, 0),
        12,
        "never the departure bell or the next"
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

// ------------------------------------------------ review fixes (K1, 2026-09-27)

/// Split and merge use checked arithmetic: a huge split is refused in
/// release builds too (it used to wrap and mint troops).
#[test]
fn split_and_merge_are_checked_at_the_edges() {
    let mut h = Host::muster(1, 1, 0, UnitType::Spearman, MAX_HOST_TROOPS, 0).unwrap();
    for t in [
        u32::MAX,
        u32::MAX - 50_000,
        MAX_HOST_TROOPS,
        MAX_HOST_TROOPS - 1,
    ] {
        assert_eq!(h.split(t, 2, 1, 1), Err(HostError::TooSmall), "{t}");
    }
    assert_eq!(
        h.split(MIN_HOST_TROOPS - 1, 2, 1, 1),
        Err(HostError::TooSmall)
    );
    assert_eq!((h.troops, h.pending), (MAX_HOST_TROOPS, None));
    let mut tiny = Host::muster(3, 1, 0, UnitType::Spearman, MIN_HOST_TROOPS, 0).unwrap();
    assert_eq!(
        tiny.split(MIN_HOST_TROOPS, 4, 1, 1),
        Err(HostError::TooSmall),
        "both keep ≥ 100 troops"
    );
    h.split(MAX_HOST_TROOPS - MIN_HOST_TROOPS, 2, 1, 1).unwrap();
    h.apply_clash(1, MAX_HOST_TROOPS, STAMINA_CAP, false)
        .unwrap();
    let c = h.settle(2).unwrap().unwrap();
    assert_eq!(h.troops + c.troops, MAX_HOST_TROOPS, "no troops created");
    assert_eq!(h.troops, MIN_HOST_TROOPS);
    let mut big = Host {
        troops: u32::MAX,
        ..Host::muster(5, 1, 0, UnitType::Spearman, MIN_HOST_TROOPS, 0).unwrap()
    };
    let mut other = Host::muster(6, 1, 0, UnitType::Spearman, MAX_HOST_TROOPS, 0).unwrap();
    assert_eq!(big.merge(&mut other, 1, 1), Err(HostError::TooLarge));
}

/// A siege advances only while the faction that blew its horn holds the
/// hex: a third faction taking the hex does not advance it (§6.3).
#[test]
fn a_siege_advances_only_while_its_declarer_holds() {
    let t = flat();
    let g = [Garrison {
        id: 700,
        faction: 0,
        tile: 30,
        troops: 0,
        walls: false,
        posture: Posture::default(),
    }];
    let a = [fighter(5, 2, UnitType::Knight, 25_000, 30)];
    let o = clash(&t, 1, &[], &g, &a, Relations::ALL_HOSTILE);
    let gr = o.garrisons[0];
    assert!(gr.attackers_hold);
    assert_eq!(gr.holders, 1 << 2);
    let report = BellReport {
        holders: gr.holders,
        defender_present: gr.defender_present,
    };
    let vigil = Vigil::new(12 * 3_600).unwrap(); // bell 1 (00:10 UTC) is outside
    let mut third = Siege::declare(3, 0, 0, 0);
    third
        .advance(1, bell_start(GENESIS, 1), report, &vigil)
        .unwrap();
    assert_eq!((third.progress, third.held), (0, false));
    let mut own = Siege::declare(2, 0, 0, 0);
    own.advance(1, bell_start(GENESIS, 1), report, &vigil)
        .unwrap();
    assert_eq!((own.progress, own.held), (1, true));
    // Once it has held the hex, losing it to a third faction fails it.
    let lost = BellReport {
        holders: 1 << 4,
        defender_present: false,
    };
    assert_eq!(
        own.advance(2, bell_start(GENESIS, 2), lost, &vigil)
            .unwrap(),
        SiegeStatus::Failed
    );
}

/// Fair share by side (§6.1): allied factions cannot pool slots against a
/// third, on a holding's hex or on open ground.
#[test]
fn allies_cannot_pool_hex_slots() {
    let t = flat();
    let mut rel = Relations::ALL_HOSTILE;
    rel.set_peaceful(0, 1, true);
    let g = [Garrison {
        id: 500,
        faction: 0,
        tile: 40,
        troops: 1_000_000,
        walls: false,
        posture: Posture::default(),
    }];
    // The owner (0) and two hosts of its ally (1) against three of faction 2.
    let res = [
        fighter(10, 0, UnitType::Spearman, 100, 40),
        fighter(11, 1, UnitType::Spearman, 100, 40),
        fighter(12, 1, UnitType::Spearman, 100, 40),
    ];
    let arr: Vec<Fighter> = (0..3)
        .map(|i| fighter(60 + i, 2, UnitType::Knight, 20_000, 40))
        .collect();
    let o = clash(&t, 2, &res, &g, &arr, rel);
    for f in &arr {
        assert!(
            o.fighter(f.id).unwrap().engaged,
            "attacker {} admitted",
            f.id
        );
    }
    for f in &res {
        assert!(
            o.fighter(f.id).unwrap().engaged,
            "owner side {} admitted",
            f.id
        );
    }
    // Open ground: six defenders of faction 0; five mutually peaceful
    // factions send one host each. They are one side: 3 slots, not 5.
    let mut coalition = Relations::ALL_HOSTILE;
    for a in 1..=5u8 {
        for b in 1..=5u8 {
            coalition.set_peaceful(a, b, true);
        }
    }
    let defenders: Vec<Fighter> = (0..6)
        .map(|i| fighter(10 + i, 0, UnitType::Spearman, 1_000 + 100 * i as u32, 40))
        .collect();
    let arr: Vec<Fighter> = (1..=5u8)
        .map(|f| fighter(60 + f as u64, f, UnitType::Spearman, 900, 40))
        .collect();
    let o = clash(&t, 3, &defenders, &[], &arr, coalition);
    let kept = defenders
        .iter()
        .filter(|f| o.fighter(f.id).unwrap().engaged)
        .count();
    let admitted = arr
        .iter()
        .filter(|f| o.fighter(f.id).unwrap().engaged)
        .count();
    assert_eq!((kept, admitted), (3, 3));
    // All hostile: five factions, five sides, each guaranteed one slot.
    let o = clash(&t, 3, &defenders, &[], &arr, Relations::ALL_HOSTILE);
    let admitted = arr
        .iter()
        .filter(|f| o.fighter(f.id).unwrap().engaged)
        .count();
    assert_eq!(admitted, 5);
}

/// Every site and every gate is in one passable component of the map, over
/// many ring seeds (the review found 10.3% of provinces with split gates
/// and 1.74% of sites on islands before paths were carved).
#[test]
fn every_site_and_gate_is_connected_across_the_map() {
    use std::collections::{BTreeMap, BTreeSet, VecDeque};
    let centre = tile_index(Hex::ORIGIN).unwrap();
    for s in 0..24u64 {
        let ring_seed = |p: ProvinceCoord| seed(b"ring", s * 1_000 + p.ring() as u64);
        let n = provinces_within(6);
        let land: BTreeMap<ProvinceCoord, ProvinceTerrain> = (0..n)
            .map(|i| {
                let p = ProvinceCoord::from_index(i);
                (p, generate_province(&ring_seed(p), p))
            })
            .collect();
        let passable = |h: Hex| {
            let (p, i) = locate(h);
            land.get(&p).is_some_and(|t| t.passable(i))
        };
        let start = ProvinceCoord::CONCORD.tile(centre).unwrap();
        assert!(passable(start));
        let mut seen: BTreeSet<(i32, i32)> = BTreeSet::new();
        let mut queue = VecDeque::from([start]);
        seen.insert((start.q, start.r));
        while let Some(h) = queue.pop_front() {
            for nb in h.neighbors() {
                if passable(nb) && seen.insert((nb.q, nb.r)) {
                    queue.push_back(nb);
                }
            }
        }
        for (p, t) in &land {
            let reach = |i: u8| {
                let h = p.tile(i).unwrap();
                seen.contains(&(h.q, h.r))
            };
            for i in &t.sites[..t.site_count as usize] {
                assert!(reach(*i), "seed {s}: site {i} of {p:?}");
            }
            for g in GATES.iter().flatten() {
                assert!(reach(*g), "seed {s}: gate {g} of {p:?}");
            }
        }
    }
}

/// The clash keeps both halves of `combat::resolve_engagement`: a garrison
/// never attacks and retaliates at v9's City rate (×0.5), and a ranged
/// defender retaliates at ×0.5 (`MELEE_VS_RANGED_RETALIATION`).
#[test]
fn engagements_keep_the_v9_retaliation_rules() {
    use permutation_rules::combat::{
        damage, modifier, resolve_engagement, variance, Combatant, Situation,
    };
    use permutation_rules::rng::rand;
    use permutation_rules::units::stats;
    let rules = frontier_ruleset();
    let t = flat();
    let key = |city: bool, id: u64| {
        let mut k = [0u8; 9];
        k[0] = city as u8;
        k[1..].copy_from_slice(&id.to_le_bytes());
        k
    };
    let dice = |bell: u32, a: [u8; 9], d: [u8; 9]| {
        let cs = clash_seed(&seed(b"bell", bell as u64), P, bell);
        let mut id = [0u8; 18];
        id[..9].copy_from_slice(&a);
        id[9..].copy_from_slice(&d);
        let eid = rand(&cs, b"eng", &id) as u32;
        (variance(&rules, &cs, eid, 0), variance(&rules, &cs, eid, 1))
    };
    // A Knight host against a garrison of 2,000 without walls.
    let bell = 3;
    let k = fighter(5, 2, UnitType::Knight, 20_000, 30);
    let g = Garrison {
        id: 700,
        faction: 0,
        tile: 30,
        troops: 2_000 * 1000,
        walls: false,
        posture: Posture::default(),
    };
    let o = clash(&t, bell, &[], &[g], &[k], Relations::ALL_HOSTILE);
    assert_eq!(o.engagements, 1, "the garrison never attacks");
    let (va, vd) = dice(bell, key(false, 5), key(true, 700));
    let knight = Combatant::Army {
        unit: UnitType::Knight,
        troops: 20_000 * 1000,
    };
    let city = Combatant::City {
        defense: 2_000 * 1000,
    };
    let (to_def, to_att) = resolve_engagement(&rules, knight, city, Situation::default(), va, vd);
    assert_eq!(o.fighter(5).unwrap().troops, 20_000 * 1000 - to_att);
    assert_eq!(
        o.garrisons[0].troops,
        (2_000 * 1000u32).saturating_sub(to_def)
    );
    let strength = stats(UnitType::Knight).strength;
    let with = damage(
        &rules,
        2_000_000,
        10,
        20_000_000,
        strength,
        &[modifier::CITY_RETALIATION],
        vd,
    );
    let without = damage(&rules, 2_000_000, 10, 20_000_000, strength, &[], vd);
    assert_eq!(to_att, with);
    assert!(
        to_att * 2 <= without + 1 && without <= to_att * 2 + 1,
        "{to_att} vs {without}"
    );
    // A resident Archer attacked by a Spearman arrival retaliates at ×0.5.
    let archer = fighter(8, 0, UnitType::Archer, 5_000, 31);
    let spear = fighter(9, 1, UnitType::Spearman, 5_000, 31);
    let o = clash(&t, bell, &[archer], &[], &[spear], Relations::ALL_HOSTILE);
    let (va, vd) = dice(bell, key(false, 9), key(false, 8));
    let s = Combatant::Army {
        unit: UnitType::Spearman,
        troops: 5_000 * 1000,
    };
    let a = Combatant::Army {
        unit: UnitType::Archer,
        troops: 5_000 * 1000,
    };
    let (to_archer, to_spear) = resolve_engagement(&rules, s, a, Situation::default(), va, vd);
    assert_eq!(
        o.fighter(8).unwrap().troops,
        (5_000 * 1000u32).saturating_sub(to_archer)
    );
    assert_eq!(
        o.fighter(9).unwrap().troops,
        (5_000 * 1000u32).saturating_sub(to_spear)
    );
    let mut mods = Vec::new();
    if permutation_rules::units::counters(UnitType::Archer, UnitType::Spearman) {
        mods.push(modifier::COUNTER);
    }
    let without = damage(
        &rules,
        5_000_000,
        stats(UnitType::Archer).strength,
        5_000_000,
        stats(UnitType::Spearman).strength,
        &mods,
        vd,
    );
    mods.push(modifier::MELEE_VS_RANGED_RETALIATION);
    let with = damage(
        &rules,
        5_000_000,
        stats(UnitType::Archer).strength,
        5_000_000,
        stats(UnitType::Spearman).strength,
        &mods,
        vd,
    );
    assert_eq!(to_spear, with);
    assert!(
        to_spear * 2 <= without + 1,
        "ranged retaliation halved: {to_spear} vs {without}"
    );
}

fn identity(o: &ClashOutcome, inp: &ClashInput) -> bool {
    o.engagements == 0
        && o.fighters.iter().all(|f| {
            inp.residents.iter().any(|r| {
                r.id == f.id
                    && f.fate == Fate::Stays { tile: r.tile }
                    && (f.troops, f.stamina, f.engaged) == (r.troops, r.stamina, false)
            })
        })
        && o.garrisons.iter().all(|g| {
            inp.garrisons
                .iter()
                .any(|x| x.id == g.id && x.troops == g.troops)
        })
}

/// A quiet bell (no arrivals, nothing to fight or bounce) changes nothing
/// under any seed, and any other bell without arrivals changes something
/// under every seed: the skip rule is seed-independent.
#[test]
fn quiet_bells_change_nothing() {
    let rules = frontier_ruleset();
    let mut rng = Rng(77);
    let (mut quiet, mut loud) = (0, 0);
    for _ in 0..400 {
        let s = random_scenario(&mut rng);
        let inp = |bell: u32| ClashInput {
            province: P,
            bell,
            seed: seed(b"bell", bell as u64),
            terrain: &s.terrain,
            residents: &s.residents,
            garrisons: &s.garrisons,
            arrivals: &[],
            relations: s.relations,
            occupancy: Occupancy::EMPTY,
        };
        let q = is_quiet(&rules, &inp(9)).unwrap();
        for bell in [9, 10, 11] {
            let o = resolve_clash(&rules, &inp(bell)).unwrap();
            assert_eq!(identity(&o, &inp(bell)), q, "bell {bell}");
        }
        if q {
            quiet += 1;
        } else {
            loud += 1;
        }
    }
    assert!(quiet > 20 && loud > 20, "{quiet} quiet, {loud} loud");
    let with_arrival = ClashInput {
        province: P,
        bell: 1,
        seed: seed(b"bell", 1),
        terrain: &flat(),
        residents: &[],
        garrisons: &[],
        arrivals: &[fighter(1, 0, UnitType::Scout, 100, 3)],
        relations: Relations::ALL_HOSTILE,
        occupancy: Occupancy::EMPTY,
    };
    assert!(!is_quiet(&rules, &with_arrival).unwrap());
}

/// `Siege::advance_quiet` over a run of quiet bells equals advancing bell
/// by bell, for any vigil (including a changed one), walls and report.
#[test]
fn quiet_runs_count_like_single_bells() {
    let mut rng = Rng(5);
    let mut completed = 0;
    for case in 0..400 {
        let mut vigil = Vigil::new(rng.below(86_400) as u32).unwrap();
        if rng.chance(50) {
            vigil
                .request_change(
                    GENESIS + rng.below(6 * DAY as u64) as i64,
                    rng.below(86_400) as u32,
                )
                .unwrap();
        }
        let f = rng.below(6) as u8;
        let declared = rng.below(1_000) as u32;
        let mut a = Siege::declare(f, declared, rng.below(500) as u32, rng.below(13) as u32);
        let mut b = a;
        let mut at = declared;
        while a.status == SiegeStatus::Active {
            let report = BellReport {
                holders: if rng.chance(85) {
                    1 << f
                } else {
                    1 << ((f + 1) % 6)
                },
                defender_present: rng.chance(15),
            };
            let span = if rng.chance(50) { 20 } else { 400 };
            let len = 1 + rng.below(span) as u32;
            let to = at + len;
            a.advance_quiet(to, GENESIS, report, &vigil).unwrap();
            for x in at + 1..=to {
                if b.status != SiegeStatus::Active {
                    break;
                }
                b.advance(x, bell_start(GENESIS, x), report, &vigil)
                    .unwrap();
            }
            assert_eq!(a, b, "case {case}");
            at = to;
        }
        completed += (a.status == SiegeStatus::Completed) as u32;
    }
    assert!(completed > 50, "only {completed} completed");
}

// ------------------------------------------------------------ quota fairness (§6.2, §9.4)

fn reveal_all(order: &[SlotEntry], bell: u32) -> (FactionSlots, Vec<SlotDecision>) {
    let mut slots: FactionSlots = [None; FACTION_ARRIVAL_SLOTS];
    let mut ds = Vec::new();
    for x in order {
        let before = slots;
        let d = admit_arrival(P, bell, &slots, *x);
        apply_slot(&mut slots, *x, d);
        // Each Reveal writes at most one slot.
        let written = (0..FACTION_ARRIVAL_SLOTS)
            .filter(|&i| slots[i] != before[i])
            .count();
        assert!(written <= 1, "a reveal wrote {written} slots");
        // One arrival per citizen, never two slots for one host.
        let mut cit: Vec<u64> = slots.iter().flatten().map(|e| e.citizen).collect();
        cit.sort_unstable();
        let n = cit.len();
        cit.dedup();
        assert_eq!(cit.len(), n, "a citizen holds two slots");
        ds.push(d);
    }
    (slots, ds)
}

fn slot_set(slots: &FactionSlots) -> Vec<SlotEntry> {
    let mut v: Vec<SlotEntry> = slots.iter().flatten().copied().collect();
    v.sort_by_key(|e| e.host_id);
    v
}

#[test]
fn arrival_slots_are_the_four_largest_regardless_of_reveal_order() {
    let mut rng = Rng(0x5107);
    let mut displaced = 0;
    for case in 0..400 {
        let bell = 1 + rng.below(4_000) as u32;
        let n = 1 + rng.below(14) as usize;
        let citizens = 1 + rng.below(8);
        // Few troop values, so ties are common and the tie key matters.
        let arrivals: Vec<SlotEntry> = (0..n)
            .map(|i| SlotEntry {
                host_id: 1_000 * case + i as u64,
                citizen: rng.below(citizens),
                troops: (100 + 100 * rng.below(6) as u32) * 1000,
            })
            .collect();
        let want = quota_set(P, bell, &arrivals);
        assert!(want.len() <= FACTION_ARRIVAL_SLOTS);
        for _ in 0..8 {
            let mut order = arrivals.clone();
            rng.shuffle(&mut order);
            let (slots, ds) = reveal_all(&order, bell);
            assert_eq!(slot_set(&slots), want, "case {case}");
            displaced += ds
                .iter()
                .filter(|d| matches!(d, SlotDecision::Displace { .. }))
                .count();
        }
        // The kept set really is the largest: nothing refused or displaced
        // outranks a kept arrival of another citizen.
        let min_kept = want.iter().map(|e| e.troops).min().unwrap_or(0);
        for a in &arrivals {
            if want.len() == FACTION_ARRIVAL_SLOTS && !want.iter().any(|e| e.citizen == a.citizen) {
                assert!(
                    a.troops <= min_kept,
                    "case {case}: a larger arrival was left out"
                );
            }
        }
    }
    assert!(
        displaced > 500,
        "displacement barely exercised: {displaced}"
    );
}

#[test]
fn a_spy_cannot_squat_the_slots_and_a_citizen_gets_one() {
    let e = |host_id, citizen, troops: u32| SlotEntry {
        host_id,
        citizen,
        troops: troops * 1000,
    };
    // A spy fills all four slots with 100-troop hosts first.
    let mut order: Vec<SlotEntry> = (0..4).map(|i| e(10 + i, 99, 100)).collect();
    order.extend([e(1, 1, 3_000), e(2, 2, 2_000), e(3, 3, 900), e(4, 4, 101)]);
    let (slots, ds) = reveal_all(&order, 7);
    assert_eq!(
        slot_set(&slots),
        vec![e(1, 1, 3_000), e(2, 2, 2_000), e(3, 3, 900), e(4, 4, 101)]
    );
    // The spy's second to fourth hosts compete with the spy's own slot.
    assert!(ds[1..4].iter().all(
        |d| *d == SlotDecision::Refuse(SlotRefusal::CitizenHasLarger)
            || matches!(d, SlotDecision::Displace { displaced, .. } if displaced.citizen == 99)
    ));
    // A citizen's larger host replaces the smaller one; a repeat is refused.
    let mut s: FactionSlots = [None; FACTION_ARRIVAL_SLOTS];
    for x in [e(1, 5, 200), e(2, 5, 500)] {
        let d = admit_arrival(P, 7, &s, x);
        apply_slot(&mut s, x, d);
    }
    assert_eq!(slot_set(&s), vec![e(2, 5, 500)]);
    assert_eq!(
        admit_arrival(P, 7, &s, e(2, 5, 500)),
        SlotDecision::Refuse(SlotRefusal::AlreadyIn)
    );
    assert_eq!(
        admit_arrival(P, 7, &s, e(3, 5, 400)),
        SlotDecision::Refuse(SlotRefusal::CitizenHasLarger)
    );
}

// ------------------------------------------------------------ seed-round margin (§8.5)

#[test]
fn the_seed_round_is_after_the_reveal_close_plus_the_margin() {
    let mut rng = Rng(0x5eed);
    let clocks = [
        QUICKNET,
        BeaconClock {
            genesis: 1_727_521_075,
            period: 3,
        },
        BeaconClock {
            genesis: 1_595_431_050,
            period: 30,
        },
    ];
    for clock in clocks {
        for _ in 0..20_000 {
            let anchor = clock.genesis + rng.below(400_000_000) as i64 - 1_000;
            let close = reveal_close(anchor);
            assert_eq!(close, anchor + REVEAL_WINDOW_SECS);
            let s = seed_round(&clock, anchor);
            let t = clock.round_time(s);
            // The seed's round is scheduled no earlier than close + M, and
            // it is the first such round.
            assert!(t >= close + SEED_MARGIN_SECS, "{clock:?} anchor {anchor}");
            assert!(t < close + SEED_MARGIN_SECS + clock.period);
            assert!(s == 1 || clock.round_time(s - 1) < close + SEED_MARGIN_SECS);
            // A beacon appears after its scheduled time, so the seed is
            // unknown to everyone until at least M after the close.
            let published = t + 1;
            assert!(published > close + SEED_MARGIN_SECS);
            // Reveals: open just before the close while the seed round is
            // not on chain; closed by the clock or by any round ≥ S.
            assert!(reveal_open(&clock, close - 1, anchor, s - 1));
            assert!(!reveal_open(&clock, close, anchor, 0));
            assert!(!reveal_open(&clock, anchor, anchor, s));
            assert!(!reveal_open(&clock, anchor, anchor, s + 1_000));
        }
    }
}

// ------------------------------------------------------------ doctrines (§4.1, O5)

#[test]
fn doctrines_are_asymmetric_bounded_and_never_multiply_a_scored_fact() {
    assert_eq!(validate_table(&DOCTRINES), Ok(()));
    // Three drilled doctrines cover the stance cycle once each.
    let mut drills: Vec<Stance> = DOCTRINES
        .iter()
        .filter_map(|d| d.drill.map(|x| x.0))
        .collect();
    drills.sort();
    assert_eq!(drills, vec![Stance::Assault, Stance::Flank, Stance::Brace]);
    // The drill applies only in its stance, the variant weight in every
    // stance, neither in Disarray.
    for d in &DOCTRINES {
        for s in STANCES {
            let m = d.dealt_bps(Posture::Stance(s), false);
            match d.drill {
                Some((ds, bps)) if ds == s => {
                    assert_eq!(
                        m,
                        (bps as u64 * d.variant_bps as u64 / BPS_ONE as u64) as u32
                    )
                }
                _ => assert_eq!(m, d.variant_bps),
            }
        }
        assert_eq!(d.dealt_bps(Posture::Disarray, false), BPS_ONE);
        assert_eq!(
            d.dealt_bps(Posture::Stance(Stance::Hold), true),
            (d.arrival_bps as u64 * d.variant_bps as u64 / BPS_ONE as u64) as u32
        );
        // Out-of-supply attrition never exceeds the kernel's.
        let after = supply_attrition(100_000, 6);
        assert!(d.attrition(100_000, after) >= after);
        assert!(d.attrition(100_000, after) <= 100_000);
    }
    let a = doctrine::of_faction(0).unwrap();
    assert_eq!(a.siege_bells(100, true), required_bells(100, 0) + 12);
    assert_eq!(a.siege_bells(100, false), required_bells(100, 0));
    // The bounds bite.
    let strong = Doctrine {
        drill: Some((Stance::Assault, 13_000)),
        ..DOCTRINES[2]
    };
    assert_eq!(strong.validate(), Err(DoctrineError::Combat));
    let hold = Doctrine {
        drill: Some((Stance::Hold, 10_500)),
        ..DOCTRINES[2]
    };
    assert_eq!(hold.validate(), Err(DoctrineError::HoldDrill));
    let free = Doctrine {
        upkeep_bps: 1_000,
        ..DOCTRINES[5]
    };
    assert_eq!(free.validate(), Err(DoctrineError::Discount));
    let mut twins = DOCTRINES;
    twins[1] = twins[0];
    assert_eq!(validate_table(&twins), Err(DoctrineError::Shape));
    // No two doctrines field the same army: F's heavy cavalry is not B's
    // light cavalry, whatever their unsimulated knobs.
    assert_ne!(DOCTRINES[1].variant_bps, DOCTRINES[5].variant_bps);
    let mut same_army = DOCTRINES;
    same_army[5] = Doctrine {
        variant_bps: BPS_ONE,
        ..DOCTRINES[5]
    };
    assert_eq!(validate_table(&same_army), Err(DoctrineError::SameArmy));
    let heavy = Doctrine {
        variant_bps: 12_000,
        ..DOCTRINES[5]
    };
    assert_eq!(heavy.validate(), Err(DoctrineError::Combat));
    let mut bare = DOCTRINES;
    bare[4] = Doctrine {
        name: "E bare",
        ..doctrine::NEUTRAL
    };
    assert_eq!(validate_table(&bare), Err(DoctrineError::Shape));
}
