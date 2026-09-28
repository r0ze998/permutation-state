//! The Phase A clash kernel (`clash::resolve_clash`, M1 lab
//! `m1/lab/clash-opt/phaseA.patch`) against the kept reference
//! (`clash::resolve_clash_ref`, the original step-by-step body with the
//! same rules), promoted from the lab's `host/tests/equiv.rs` (I-14, W1-A):
//!
//! * `phase_a_equals_the_reference_on_4320_inputs`: the lab's 4,320 inputs
//!   (6 adversarial fill kinds × 120 seeds × 6 variants: every wedge,
//!   symmetric and raw asymmetric relations, retreat orders, postures,
//!   thinned fills), outcome and digest identical, with `Occupancy::EMPTY`;
//! * `phase_a_equals_the_reference_with_occupancy`: the same fills with a
//!   random storage room (pending musters, departed entries, I-43);
//! * `phase_a_equals_the_reference_on_the_refund_corner`: an engaged
//!   faction that deals and takes 0 on its hex still pays its stamina
//!   (the refund's `d > 0` guard, I-42: the lab's mutation survivor);
//! * `phase_a_equals_the_reference_on_ties_and_edges`: 5,000 small fills
//!   built to reach the tie-breaks (equal strengths on a hex contested
//!   without a fight, stamina around `ENGAGE_STAMINA`), which the dense
//!   fills never do;
//! * `occupancy_empty_keeps_the_phase_b_digests`: the dense fills with
//!   four residents per faction and no civilians (so neither the cap
//!   recount, nor the scout rule, nor the storage room can apply) give
//!   the recorded Phase B outcomes (through wave 5: exactly those of the
//!   kernel at `d95fa25`; Phase B, `CLASH_VERSION` 3, moved only the
//!   variance stream);
//! * `m1_rules_keep_the_phase_b_digests`: the M1 rules over the same
//!   fills, empty and random room (recorded at `31f1aa1` until Phase B).
//!
//! Both bodies carry Phase B (one hash per engagement for both dice), so
//! every equivalence test compares Phase B with Phase B.

use permutation_rules::fixed::BPS_ONE;
use permutation_rules::frontier::clash::{
    frontier_ruleset, resolve_clash, resolve_clash_ref, ClashError, ClashInput, ClashOutcome, Fate,
    Fighter, Garrison, Occupancy, Relations, FACTION_LIMIT,
};
use permutation_rules::frontier::geometry::{ProvinceCoord, PROVINCE_TILES};
use permutation_rules::frontier::host::{
    ENGAGE_STAMINA, FACTION_RESIDENT_CAP, PROVINCE_HOST_CAP, STAMINA_CAP,
};
use permutation_rules::frontier::stance::{posture_of, Posture, Stance};
use permutation_rules::frontier::terrain::{generate_province, ProvinceTerrain};
use permutation_rules::hash::sha256;
use permutation_rules::params::Ruleset;
use permutation_rules::units::UnitType;

// ------------------------------------------------------------ fills (lab copy)
// BEGIN FILLS (the M1 lab's generator, byte for byte in behaviour)

/// xorshift64, as in the lab.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
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

#[derive(Clone)]
struct Scenario {
    terrain: ProvinceTerrain,
    residents: Vec<Fighter>,
    garrisons: Vec<Garrison>,
    arrivals: Vec<Fighter>,
}

/// SP-V2's adversarial fills (`host/src/world.rs`). `kind`: 0 "dense"
/// (6 factions × 8 residents, 6 per hex on 8 hexes, a garrison on each
/// contested hex, 24 arrivals onto the same hexes); 1 "pile-up" (as 0, all
/// arrivals onto 2 hexes); 2 random (≤ 6 residents per hex on 8–12 hexes);
/// 3 "spread12" (12 hexes, 4 residents + 2 arrivals of 6 distinct factions
/// on each, no retreat orders, garrisons off the contested hexes); 4 as 3
/// with a NEUTRAL garrison on every contested hex; 5 random over 8–14
/// hexes, garrison factions 0..=6 anywhere.
#[allow(clippy::needless_range_loop)] // kept as the lab wrote it
fn adversarial(kind: u8, seed: u64, p: i32, q: i32) -> Scenario {
    let terrain = generate_province(&sha256(&[&seed.to_le_bytes()]), ProvinceCoord::new(p, q));
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let passable: Vec<u8> = (0..PROVINCE_TILES as u8)
        .filter(|&t| terrain.passable(t))
        .collect();
    assert!(passable.len() >= 20, "too little passable land");
    let k = match kind {
        2 => 8 + rng.below(5) as usize,
        3 | 4 => 12,
        5 => 8 + rng.below(7) as usize,
        _ => 8,
    };
    let step = passable.len() / k;
    let hexes: Vec<u8> = (0..k).map(|j| passable[j * step]).collect();
    let troops = |r: &mut Rng| (5_000 + r.below(25_000) as u32) * 1000;
    let mut residents = vec![];
    let mut per_hex = vec![0usize; k];
    for f in 0..6u8 {
        let spread: Vec<usize> = (0..k)
            .filter(|&h| (f as usize + 6 - h % 6) % 6 < 4)
            .collect();
        for n in 0..8usize {
            let h = match kind {
                2 | 5 => loop {
                    let h = rng.below(k as u64) as usize;
                    if per_hex[h] < 6 {
                        break h;
                    }
                },
                3 | 4 => spread[n],
                _ => n,
            };
            per_hex[h] += 1;
            residents.push(Fighter {
                id: 1000 + (f as u64) * 8 + n as u64,
                faction: f,
                unit: UNITS[rng.below(7) as usize],
                troops: troops(&mut rng),
                stamina: 60 + rng.below(61) as u16,
                tile: hexes[h],
                posture: Posture::default(),
                retreat_bps: None,
                dealt_bps: BPS_ONE,
            });
        }
    }
    let mut gtiles: Vec<u8> = if kind == 3 { vec![] } else { hexes.clone() };
    let mut pool: Vec<u8> = passable.clone();
    if kind == 5 {
        gtiles.clear();
        for j in (1..pool.len()).rev() {
            pool.swap(j, rng.below(j as u64 + 1) as usize);
        }
    }
    for t in pool.iter() {
        if gtiles.len() >= 12 {
            break;
        }
        if !gtiles.contains(t) && (kind != 3 || !hexes.contains(t)) {
            gtiles.push(*t);
        }
    }
    gtiles.truncate(12);
    let gfac: Vec<u8> = (0..gtiles.len())
        .map(|j| match kind {
            4 => 6,
            5 => rng.below(7) as u8,
            _ => (j % 6) as u8,
        })
        .collect();
    let garrisons: Vec<Garrison> = gtiles
        .iter()
        .enumerate()
        .map(|(j, &t)| Garrison {
            id: 5000 + j as u64,
            faction: gfac[j],
            tile: t,
            troops: (2_000 + rng.below(8_000) as u32) * 1000,
            walls: j % 3 == 0,
            posture: Posture::default(),
        })
        .collect();
    let mut arrivals = vec![];
    for f in 0..6u8 {
        let spread: Vec<usize> = (0..k)
            .filter(|&h| (f as usize + 6 - h % 6) % 6 >= 4)
            .collect();
        for i in 0..4u8 {
            let n = (f as usize) * 4 + i as usize;
            let tile = match kind {
                1 => hexes[n % 2],
                2 | 5 => hexes[rng.below(k as u64) as usize],
                3 | 4 => hexes[spread[i as usize]],
                _ => hexes[n % 8],
            };
            let retreat = match (kind, n % 4) {
                (3 | 4, _) => None,
                (_, 0) => Some(5_000 + rng.below(20_000) as u32),
                (_, 1) => Some(60_000),
                _ => None,
            };
            arrivals.push(Fighter {
                id: 2000 + n as u64,
                faction: f,
                unit: UNITS[rng.below(7) as usize],
                troops: troops(&mut rng),
                stamina: 40 + rng.below(81) as u16,
                tile,
                posture: Posture::Stance(Stance::from_u8(rng.below(4) as u8).unwrap()),
                retreat_bps: retreat,
                dealt_bps: BPS_ONE,
            });
        }
    }
    Scenario {
        terrain,
        residents,
        garrisons,
        arrivals,
    }
}

fn posture(r: &mut Rng) -> Posture {
    match r.below(4) {
        0 => posture_of(false, None),
        1 => Posture::Disarray,
        _ => posture_of(true, Stance::from_u8(r.below(4) as u8)),
    }
}

/// One of the lab's 4,320 inputs, before the province, bell and seed are
/// attached.
struct Case {
    kind: u8,
    seed: u64,
    variant: u32,
    p: ProvinceCoord,
    sc: Scenario,
    relations: Relations,
    seed32: [u8; 32],
}

/// The lab's 4,320 inputs (6 kinds × 120 seeds × 6 variants), in order.
fn lab_cases(mut each: impl FnMut(&Case)) {
    for kind in 0..6u8 {
        for seed in 1..=120u64 {
            let mut rng = Rng(seed.wrapping_mul(0xA24B_AED4_963E_E407)
                ^ (kind as u64 + 1).wrapping_mul(0x9FB2_1C65_1E98_DF25)
                | 1);
            let (p, q) =
                [(20, -3), (3, 20), (-17, 23), (-20, 3), (-3, -20), (17, -23)][(seed % 6) as usize];
            let base = adversarial(kind, seed, p, q);
            for variant in 0..6u32 {
                let mut sc = base.clone();
                for f in sc.residents.iter_mut() {
                    f.posture = posture(&mut rng);
                }
                for g in sc.garrisons.iter_mut() {
                    g.posture = posture(&mut rng);
                }
                for a in sc.arrivals.iter_mut() {
                    a.posture = posture_of(true, Stance::from_u8(rng.below(4) as u8));
                    a.retreat_bps = if rng.below(3) == 0 {
                        Some(5_000 + rng.below(30_000) as u32)
                    } else {
                        None
                    };
                    if rng.below(5) == 0 {
                        a.troops = 100_000 + rng.below(2_000_000) as u32;
                    }
                }
                if variant >= 3 {
                    sc.residents.retain(|_| rng.below(3) != 0);
                    sc.arrivals.retain(|_| rng.below(2) != 0);
                    sc.garrisons.retain(|_| rng.below(2) != 0);
                }
                let relations = match variant % 3 {
                    0 => Relations::ALL_HOSTILE,
                    1 => {
                        let mut r = Relations::ALL_HOSTILE;
                        for a in 0..6u8 {
                            for b in a + 1..6u8 {
                                if rng.below(3) == 0 {
                                    r.set_peaceful(a, b, true);
                                }
                            }
                        }
                        r
                    }
                    _ => Relations {
                        peaceful: rng.next() & rng.next(),
                    },
                };
                let mut seed32 = [0u8; 32];
                for c in seed32.chunks_mut(8) {
                    c.copy_from_slice(&rng.next().to_le_bytes());
                }
                each(&Case {
                    kind,
                    seed,
                    variant,
                    p: ProvinceCoord::new(p, q),
                    sc,
                    relations,
                    seed32,
                });
            }
        }
    }
}

/// The dense fills without the rules M1 added on top of `d95fa25`: four
/// residents per faction (so 24 + 24 ≤ 48 and 4 + 4 ≤ 8: no province cap
/// binds, so the cap recount has nothing to re-admit), and every Scout
/// replaced by a Spearman (so the scout rule has nothing to change).
fn without_m1_rules(sc: &Scenario) -> Scenario {
    let mut s = sc.clone();
    let mut seen = [0u8; FACTION_LIMIT as usize];
    s.residents.retain(|f| {
        seen[f.faction as usize] += 1;
        seen[f.faction as usize] <= 4
    });
    for f in s.residents.iter_mut().chain(s.arrivals.iter_mut()) {
        if f.unit == UnitType::Scout {
            f.unit = UnitType::Spearman;
        }
    }
    s
}

/// Rolling digest over a sequence of results.
fn fold(acc: &mut [u8; 32], r: &Result<ClashOutcome, ClashError>) {
    let d = match r {
        Ok(o) => o.digest(),
        Err(e) => sha256(&[format!("{e:?}").as_bytes()]),
    };
    *acc = sha256(&[acc, &d]);
}

// END FILLS

fn input<'a>(c: &'a Case, sc: &'a Scenario, occupancy: Occupancy) -> ClashInput<'a> {
    ClashInput {
        province: c.p,
        bell: 1000 + c.variant,
        seed: c.seed32,
        terrain: &sc.terrain,
        residents: &sc.residents,
        garrisons: &sc.garrisons,
        arrivals: &sc.arrivals,
        relations: c.relations,
        occupancy,
    }
}

fn hex(d: &[u8; 32]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// Both kernels on one input: identical outcome, digest or error.
fn same(rules: &Ruleset, inp: &ClashInput, what: &str) -> Result<ClashOutcome, ClashError> {
    let a = resolve_clash_ref(rules, inp);
    let b = resolve_clash(rules, inp);
    match (&a, &b) {
        (Ok(x), Ok(y)) => {
            assert_eq!(x, y, "{what}");
            assert_eq!(x.digest(), y.digest(), "{what}");
        }
        (Err(x), Err(y)) => assert_eq!(x, y, "{what}"),
        _ => panic!("{what}: {a:?} vs {b:?}"),
    }
    a
}

#[test]
fn phase_a_equals_the_reference_on_4320_inputs() {
    let rules = frontier_ruleset();
    let (mut cases, mut engagements, mut withdrew, mut bounced, mut errors) =
        (0u32, 0u64, 0u64, 0u64, 0u32);
    lab_cases(|c| {
        let what = format!("kind {} seed {} variant {}", c.kind, c.seed, c.variant);
        match same(&rules, &input(c, &c.sc, Occupancy::EMPTY), &what) {
            Ok(o) => {
                engagements += o.engagements as u64;
                for f in &o.fighters {
                    match f.fate {
                        Fate::Withdrew { .. } => withdrew += 1,
                        Fate::Bounced => bounced += 1,
                        _ => {}
                    }
                }
            }
            Err(_) => errors += 1,
        }
        cases += 1;
    });
    println!(
        "EQUIV cases {cases} identical {cases} engagements {engagements} withdrawals {withdrew} bounces {bounced} errors {errors}"
    );
    assert_eq!(cases, 4_320);
    assert_eq!(errors, 0, "every lab fill is a valid input");
    assert!(
        withdrew > 1_000,
        "the withdraw search was barely exercised: {withdrew}"
    );
    assert!(engagements > 100_000, "too few engagements: {engagements}");
}

#[test]
fn phase_a_equals_the_reference_with_occupancy() {
    let rules = frontier_ruleset();
    let mut rng = Rng(0x5EED_0CC0_u64 | 1);
    let mut cases = 0u32;
    let mut room_bounced = 0u64;
    let mut honest_cases = 0u32;
    lab_cases(|c| {
        let mut honest_caps = true;
        // Thin the residents so that pending musters and departed entries
        // fit beside them, then fill the room so that it often binds.
        let mut sc = c.sc.clone();
        let keep = rng.below(49) as usize;
        sc.residents.truncate(keep);
        let r = sc.residents.len();
        let mut occ = Occupancy::EMPTY;
        let mut left = 48 - r;
        for f in 0..6usize {
            let m = (rng.below(4) as usize).min(left);
            occ.pending[f] = m as u8;
            left -= m;
        }
        let m: usize = occ.pending.iter().map(|&x| x as usize).sum();
        let d = rng.below((56 - r - m) as u64 + 1) as usize;
        occ.storage_free = (56 - r - m - d) as u8;
        let what = format!(
            "kind {} seed {} variant {} occupancy {occ:?}",
            c.kind, c.seed, c.variant
        );
        let o = same(&rules, &input(c, &sc, occ), &what).expect("valid");
        // The storage bound (I-43): entries after the resolve ≤ 56.
        let stays = o
            .fighters
            .iter()
            .filter(|f| f.arrival && matches!(f.fate, Fate::Stays { .. } | Fate::Withdrew { .. }))
            .count();
        assert!(
            stays <= occ.storage_free as usize,
            "{what}: {stays} arrivals stay"
        );
        // The two cap invariants (I-43, G8): per faction, the hosts that
        // stay (residents and arrivals) plus its pending musters ≤ 8, and
        // in all ≤ 48 counting every pending muster. The generator thins
        // residents without regard to faction, so a faction can enter
        // already over 8 with its pending musters (an input Muster never
        // builds); there the bound is that arrivals add nothing to it.
        let mut before = [0usize; FACTION_LIMIT as usize];
        for x in &sc.residents {
            before[x.faction as usize] += 1;
        }
        let mut per = [0usize; FACTION_LIMIT as usize];
        for r in &o.fighters {
            if matches!(r.fate, Fate::Stays { .. } | Fate::Withdrew { .. }) {
                let f = sc
                    .residents
                    .iter()
                    .chain(sc.arrivals.iter())
                    .find(|x| x.id == r.id)
                    .expect("known id")
                    .faction;
                per[f as usize] += 1;
            }
        }
        for f in 0..6 {
            let cap = FACTION_RESIDENT_CAP.max(before[f] + occ.pending[f] as usize);
            honest_caps &= before[f] + occ.pending[f] as usize <= FACTION_RESIDENT_CAP;
            assert!(
                per[f] + occ.pending[f] as usize <= cap,
                "{what}: faction {f} holds {} + {} pending",
                per[f],
                occ.pending[f]
            );
        }
        let total: usize = per.iter().sum::<usize>() + m;
        assert!(
            total <= PROVINCE_HOST_CAP,
            "{what}: {total} hosts counting pending musters"
        );
        room_bounced += (sc.arrivals.len() - stays) as u64;
        honest_cases += honest_caps as u32;
        cases += 1;
    });
    assert_eq!(cases, 4_320);
    assert!(honest_cases > 1_500, "{honest_cases} cases inside the caps");
    assert!(room_bounced > 1_000);
}

/// The refund's `d > 0` guard (I-42): two hostile hosts too small to hurt
/// each other both engage, deal 0 and take 0; a faction whose damage ratio
/// is 0 / 0 pays its engagement stamina like any other.
#[test]
fn phase_a_equals_the_reference_on_the_refund_corner() {
    let rules = frontier_ruleset();
    let terrain = generate_province(&sha256(&[b"refund"]), ProvinceCoord::new(3, 1));
    let tile = (0..PROVINCE_TILES as u8)
        .find(|&t| terrain.passable(t))
        .expect("land");
    let tiny = |id: u64, faction: u8| Fighter {
        id,
        faction,
        unit: UnitType::Spearman,
        troops: 1,
        stamina: STAMINA_CAP,
        tile,
        posture: Posture::default(),
        retreat_bps: None,
        dealt_bps: BPS_ONE,
    };
    let residents = [tiny(1, 0)];
    let arrivals = [tiny(2, 1)];
    for bell in 0..32u32 {
        let inp = ClashInput {
            province: ProvinceCoord::new(3, 1),
            bell,
            seed: sha256(&[b"refund-seed", &bell.to_le_bytes()]),
            terrain: &terrain,
            residents: &residents,
            garrisons: &[],
            arrivals: &arrivals,
            relations: Relations::ALL_HOSTILE,
            occupancy: Occupancy::EMPTY,
        };
        let o = same(&rules, &inp, "refund corner").expect("valid");
        assert_eq!(o.engagements, 1);
        for id in [1, 2] {
            let f = o.fighter(id).unwrap();
            assert!(f.engaged, "{id} engaged");
            assert_eq!(f.troops, 0, "{id}: took no damage, but below 0.5 troops");
            assert_eq!(
                f.stamina,
                STAMINA_CAP - ENGAGE_STAMINA,
                "{id}: dealt 0 and took 0, so no refund"
            );
        }
    }
}

/// Golden digest of `without_m1_rules` over the 4,320 inputs on the
/// **Phase B** kernel (`CLASH_VERSION` 3, W6-B; identical on W5-A's
/// independent Phase B lab tree). Until Phase B this test pinned the value
/// recorded with the kernel at `d95fa25` (before CL-01, CL-10, I-43 and
/// Phase A), `c3946cb0ac96f8848ec17741fff672cb920853720a2f0940002aba7c1a2790bd`,
/// and so showed that `Occupancy::EMPTY` and Phase A changed no outcome;
/// Phase B moves the variance stream only.
const PHASE_B_DENSE_NO_M1_RULES: &str =
    "6b0869a2125a75591e9a303e276f11de61572c0c87b0b17d1245c1b7caf1d6f0";

#[test]
fn occupancy_empty_keeps_the_phase_b_digests() {
    let rules = frontier_ruleset();
    let mut acc = [0u8; 32];
    let mut n = 0u32;
    lab_cases(|c| {
        let sc = without_m1_rules(&c.sc);
        let r = same(&rules, &input(c, &sc, Occupancy::EMPTY), "golden");
        fold(&mut acc, &r);
        n += 1;
    });
    assert_eq!(n, 4_320);
    assert_eq!(
        hex(&acc),
        PHASE_B_DENSE_NO_M1_RULES,
        "outcomes moved against the Phase B recording"
    );
}

/// Tie-breaks and edges the adversarial fills never reach (the lab's
/// mutation survivors, I-42): equal strengths on a contested hex (raw
/// asymmetric relations can make a hex contested with no engagement, so
/// the defender flag and the seeded faction key decide the field), stamina
/// at and around `ENGAGE_STAMINA`, garrisons of equal strength.
#[test]
fn phase_a_equals_the_reference_on_ties_and_edges() {
    let rules = frontier_ruleset();
    let mut rng = Rng(0x7E5_u64 | 1);
    let mut contested_without_fight = 0u32;
    for case in 0..5_000u64 {
        let p = ProvinceCoord::new(20 + (case % 6) as i32, -3);
        let terrain = generate_province(&sha256(&[b"ties", &case.to_le_bytes()]), p);
        let land: Vec<u8> = (0..PROVINCE_TILES as u8)
            .filter(|&t| terrain.passable(t))
            .collect();
        let hot = [land[0], land[land.len() / 2]];
        let stamina = [0u16, 19, 20, 21, 40, STAMINA_CAP];
        let troops = [1_000_000u32, 1_000_000, 2_000_000];
        let (mut residents, mut arrivals, mut garrisons) = (vec![], vec![], vec![]);
        let mut id = 1u64;
        for f in 0..(2 + rng.below(4)) as u8 {
            for n in 0..(1 + rng.below(3)) {
                let arrival = n > 0 && rng.below(2) == 0;
                let fi = Fighter {
                    id,
                    faction: f,
                    unit: UnitType::Spearman,
                    troops: troops[rng.below(3) as usize],
                    stamina: stamina[rng.below(6) as usize],
                    tile: hot[rng.below(2) as usize],
                    posture: if arrival {
                        Posture::Stance(Stance::Hold)
                    } else {
                        Posture::default()
                    },
                    retreat_bps: None,
                    dealt_bps: BPS_ONE,
                };
                id += 1;
                if arrival {
                    arrivals.push(fi);
                } else {
                    residents.push(fi);
                }
            }
        }
        if rng.below(3) == 0 {
            garrisons.push(Garrison {
                id: 500,
                faction: rng.below(7) as u8,
                tile: hot[0],
                troops: 100_000,
                walls: rng.below(2) == 0,
                posture: Posture::default(),
            });
        }
        let relations = Relations {
            peaceful: rng.next() & rng.next(),
        };
        let inp = ClashInput {
            province: p,
            bell: case as u32,
            seed: sha256(&[b"ties-seed", &case.to_le_bytes()]),
            terrain: &terrain,
            residents: &residents,
            garrisons: &garrisons,
            arrivals: &arrivals,
            relations,
            occupancy: Occupancy::EMPTY,
        };
        let o = same(&rules, &inp, &format!("ties case {case}")).expect("valid");
        let bounced_unhurt = o
            .fighters
            .iter()
            .any(|f| !f.engaged && f.fate == Fate::Bounced);
        contested_without_fight += (o.engagements == 0 && bounced_unhurt) as u32;
    }
    assert!(
        contested_without_fight > 50,
        "the tie-break path was barely reached: {contested_without_fight}"
    );
}

/// Golden digests of `resolve_clash` **with the M1 rules** (the lab fills
/// as they are: scouts, more than four residents per faction), recorded
/// with the kernel at `31f1aa1` — the wave-5 base, before W5-A replaced
/// the sorts of the helpers `resolve_clash` and `resolve_clash_ref` share
/// (validate, fair_share, share_hex, build_units, first_admission,
/// `RelationsLog::at`) with `sort_by_key3` (wave-5 review of W5-A). The
/// reference-vs-optimised tests above cannot see a wrong key encoding in a
/// shared helper; these digests can. `EMPTY`: `Occupancy::EMPTY`;
/// `ROOM`: the random storage room of
/// `phase_a_equals_the_reference_with_occupancy` (same RNG and draws).
///
/// **Re-recorded for Phase B** (`CLASH_VERSION` 3, W6-B): the values below
/// come out of both this kernel and W5-A's independent Phase B lab tree,
/// which still has the `31f1aa1` sorts (so the shared sort stays checked).
/// The Phase A values recorded at `31f1aa1` were EMPTY
/// `679c2abe671c503ff4cd1abd891f72b421533119081e46a3b28572eae39b163b`, ROOM
/// `de82d5204aeaf891eec3e1c60c3bee96c97d838190ca7700538573d81d8de98b`.
const M1_PHASE_B_EMPTY: &str = "523069855d2df8d73d00a9c021b33773d158c0f5fd5832cecd9f4d802a35e413";
const M1_PHASE_B_ROOM: &str = "615354a0878d281ec7f6b017f2ec03c0de5413f1fed91806270d103ce6ecb0bc";

#[test]
fn m1_rules_keep_the_phase_b_digests() {
    let rules = frontier_ruleset();
    let (mut empty, mut room) = ([0u8; 32], [0u8; 32]);
    let mut rng = Rng(0x5EED_0CC0_u64 | 1);
    let mut n = 0u32;
    lab_cases(|c| {
        fold(
            &mut empty,
            &resolve_clash(&rules, &input(c, &c.sc, Occupancy::EMPTY)),
        );
        let mut sc = c.sc.clone();
        let keep = rng.below(49) as usize;
        sc.residents.truncate(keep);
        let r = sc.residents.len();
        let mut occ = Occupancy::EMPTY;
        let mut left = 48 - r;
        for f in 0..6usize {
            let m = (rng.below(4) as usize).min(left);
            occ.pending[f] = m as u8;
            left -= m;
        }
        let m: usize = occ.pending.iter().map(|&x| x as usize).sum();
        let d = rng.below((56 - r - m) as u64 + 1) as usize;
        occ.storage_free = (56 - r - m - d) as u8;
        fold(&mut room, &resolve_clash(&rules, &input(c, &sc, occ)));
        n += 1;
    });
    assert_eq!(n, 4_320);
    println!("M1 rules: EMPTY {} ROOM {}", hex(&empty), hex(&room));
    assert_eq!(
        hex(&empty),
        M1_PHASE_B_EMPTY,
        "M1-rule outcomes moved against the Phase B recording (EMPTY)"
    );
    assert_eq!(
        hex(&room),
        M1_PHASE_B_ROOM,
        "M1-rule outcomes moved against the Phase B recording (room)"
    );
}
