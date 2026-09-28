//! Bounds and M0-closeout fixes of the clash kernel (W1-A):
//!
//! * CL-01 / CL-06 / I-27: `clash::validate` refuses troops above
//!   `MAX_HOST_TROOPS`, stamina above `STAMINA_CAP`, a doctrine multiplier
//!   outside `[BPS_ONE, COMBAT_MAX_BPS]`, a retreat ratio above
//!   `RETREAT_MAX_BPS` and a faction id above `NEUTRAL`; honest outcomes
//!   are unchanged (golden digest recorded at `d95fa25`).
//! * CL-14: the damage product is bounded at compile time and
//!   `doctrine::validate_table` refuses a table that passes the bound.
//! * CL-10: province caps are recounted after the hex fair share, and
//!   civilians (Scouts, Settlers) never contest a tile or hold it for a
//!   siege (failing-first tests, failed at `d95fa25`).
//! * I-43: the storage-aware room (`clash::Occupancy`): pending musters and
//!   departed entries bound the arrivals that stay.

use permutation_rules::fixed::{Bps, BPS_ONE};
use permutation_rules::frontier::clash::{
    frontier_ruleset, resolve_clash, ClashError, ClashInput, ClashOutcome, Fate, Fighter, Garrison,
    Occupancy, Relations, MAX_DAMAGE_PRODUCT_BPS, MAX_STANCE_BPS, NEUTRAL, RETREAT_MAX_BPS,
};
use permutation_rules::frontier::doctrine::{
    self, bounds::COMBAT_MAX_BPS, validate_table, DoctrineError, DOCTRINES,
};
use permutation_rules::frontier::geometry::{ProvinceCoord, PROVINCE_TILES};
use permutation_rules::frontier::host::{
    FACTION_RESIDENT_CAP, MAX_HOST_TROOPS, PROVINCE_HOST_CAP, STAMINA_CAP,
};
use permutation_rules::frontier::stance::{damage_bps, posture_of, Posture, Stance, STANCES};
use permutation_rules::frontier::terrain::{generate_province, ProvinceTerrain};
use permutation_rules::hash::sha256;
use permutation_rules::map::Terrain;
use permutation_rules::rng::Seed;
use permutation_rules::units::UnitType;

// ------------------------------------------------------------ helpers
// BEGIN HONEST

/// splitmix64.
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
}

const FIGHTING: [UnitType; 6] = [
    UnitType::Spearman,
    UnitType::Archer,
    UnitType::Horseman,
    UnitType::Pikeman,
    UnitType::Crossbowman,
    UnitType::Knight,
];

struct Honest {
    p: ProvinceCoord,
    bell: u32,
    seed: Seed,
    terrain: ProvinceTerrain,
    residents: Vec<Fighter>,
    garrisons: Vec<Garrison>,
    arrivals: Vec<Fighter>,
    relations: Relations,
}

/// An honest clash: fighting units only, every value inside its cap, the
/// doctrine's own multiplier, ≤ 6 residents per hex, ≤ 8 hosts per faction
/// and ≤ 48 in all counting the arrivals (so no province cap binds), up to
/// 12 garrisons on distinct tiles (factions 0..=6).
fn honest(rng: &mut Rng) -> Honest {
    let wedge = [(20, -3), (3, 20), (-17, 23), (-20, 3), (-3, -20), (17, -23)];
    let (p, q) = wedge[rng.below(6) as usize];
    let p = ProvinceCoord::new(p + rng.below(3) as i32, q);
    let terrain = generate_province(&sha256(&[b"honest", &rng.next().to_le_bytes()]), p);
    let land: Vec<u8> = (0..PROVINCE_TILES as u8)
        .filter(|&t| terrain.passable(t))
        .collect();
    let hot: Vec<u8> = (0..4)
        .map(|_| land[rng.below(land.len() as u64) as usize])
        .collect();
    let tile = |rng: &mut Rng| {
        if rng.chance(75) {
            hot[rng.below(hot.len() as u64) as usize]
        } else {
            land[rng.below(land.len() as u64) as usize]
        }
    };
    let posture =
        |rng: &mut Rng| posture_of(rng.chance(50), STANCES.get(rng.below(5) as usize).copied());
    let mut per_hex = [0u8; PROVINCE_TILES];
    let (mut residents, mut arrivals) = (Vec::new(), Vec::new());
    let mut id = 10u64;
    let factions = 2 + rng.below(5) as u8;
    for f in 0..factions {
        let d = doctrine::of_faction(f).expect("doctrine");
        let n_res = rng.below(6);
        let n_arr = rng.below(4);
        for _ in 0..n_res {
            let mut t = tile(rng);
            while per_hex[t as usize] >= 6 {
                t = land[rng.below(land.len() as u64) as usize];
            }
            per_hex[t as usize] += 1;
            id += 1 + rng.below(3);
            let posture = posture(rng);
            residents.push(Fighter {
                id,
                faction: f,
                unit: if rng.chance(60) {
                    d.unit
                } else {
                    FIGHTING[rng.below(6) as usize]
                },
                troops: (100 + rng.below(29_901) as u32) * 1000,
                stamina: rng.below(STAMINA_CAP as u64 + 1) as u16,
                tile: t,
                posture,
                retreat_bps: None,
                dealt_bps: d.dealt_bps(posture, false),
            });
        }
        for _ in 0..n_arr {
            id += 1 + rng.below(3);
            let posture = Posture::Stance(STANCES[rng.below(4) as usize]);
            arrivals.push(Fighter {
                id,
                faction: f,
                unit: if rng.chance(60) {
                    d.unit
                } else {
                    FIGHTING[rng.below(6) as usize]
                },
                troops: (100 + rng.below(29_901) as u32) * 1000,
                stamina: rng.below(STAMINA_CAP as u64 + 1) as u16,
                tile: tile(rng),
                posture,
                retreat_bps: if rng.chance(30) {
                    Some(1 + rng.below(RETREAT_CAP as u64) as Bps)
                } else {
                    None
                },
                dealt_bps: d.dealt_bps(posture, true),
            });
        }
    }
    let mut garrisons: Vec<Garrison> = Vec::new();
    for k in 0..rng.below(13) {
        let t = if rng.chance(50) {
            hot[k as usize % hot.len()]
        } else {
            tile(rng)
        };
        if garrisons.iter().any(|g| g.tile == t) {
            continue;
        }
        garrisons.push(Garrison {
            id: 90_000 + k,
            faction: rng.below(NEUTRAL as u64 + 1) as u8,
            tile: t,
            troops: rng.below(10_001) as u32 * 1000,
            walls: rng.chance(40),
            posture: posture(rng),
        });
    }
    let mut relations = Relations::ALL_HOSTILE;
    for a in 0..6u8 {
        for b in a + 1..6u8 {
            if rng.chance(15) {
                relations.set_peaceful(a, b, true);
            }
        }
    }
    let mut seed = [0u8; 32];
    for c in seed.chunks_mut(8) {
        c.copy_from_slice(&rng.next().to_le_bytes());
    }
    Honest {
        p,
        bell: rng.below(4032) as u32,
        seed,
        terrain,
        residents,
        garrisons,
        arrivals,
        relations,
    }
}

/// The retreat cap the honest fills draw below (`RETREAT_MAX_BPS`, I-27).
const RETREAT_CAP: u32 = 60_000;

fn fold(acc: &mut [u8; 32], r: &Result<ClashOutcome, ClashError>) {
    let d = match r {
        Ok(o) => o.digest(),
        Err(e) => sha256(&[format!("{e:?}").as_bytes()]),
    };
    *acc = sha256(&[acc, &d]);
}

// END HONEST

fn hex(d: &[u8; 32]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

fn run_honest(h: &Honest) -> Result<ClashOutcome, ClashError> {
    resolve_clash(
        &frontier_ruleset(),
        &ClashInput {
            province: h.p,
            bell: h.bell,
            seed: h.seed,
            terrain: &h.terrain,
            residents: &h.residents,
            garrisons: &h.garrisons,
            arrivals: &h.arrivals,
            relations: h.relations,
            occupancy: Occupancy::EMPTY,
        },
    )
}

const P: ProvinceCoord = ProvinceCoord { p: 3, q: 1 };

fn flat() -> ProvinceTerrain {
    let mut t = generate_province(&[7u8; 32], P);
    t.terrain = [Terrain::Plains; PROVINCE_TILES];
    t
}

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

fn arrival(id: u64, faction: u8, unit: UnitType, troops: u32, tile: u8) -> Fighter {
    Fighter {
        posture: Posture::Stance(Stance::Hold),
        ..fighter(id, faction, unit, troops, tile)
    }
}

fn garrison(id: u64, faction: u8, tile: u8, troops: u32) -> Garrison {
    Garrison {
        id,
        faction,
        tile,
        troops: troops * 1000,
        walls: true,
        posture: Posture::default(),
    }
}

#[allow(clippy::too_many_arguments)]
fn clash_with(
    terrain: &ProvinceTerrain,
    residents: &[Fighter],
    garrisons: &[Garrison],
    arrivals: &[Fighter],
    relations: Relations,
    occupancy: Occupancy,
) -> Result<ClashOutcome, ClashError> {
    resolve_clash(
        &frontier_ruleset(),
        &ClashInput {
            province: P,
            bell: 11,
            seed: sha256(&[b"bounds"]),
            terrain,
            residents,
            garrisons,
            arrivals,
            relations,
            occupancy,
        },
    )
}

fn clash(residents: &[Fighter], garrisons: &[Garrison], arrivals: &[Fighter]) -> ClashOutcome {
    clash_with(
        &flat(),
        residents,
        garrisons,
        arrivals,
        Relations::ALL_HOSTILE,
        Occupancy::EMPTY,
    )
    .expect("valid clash")
}

// ------------------------------------------------------------ CL-01, CL-06, I-27

#[test]
fn clash_validate_refuses_out_of_range() {
    let t = flat();
    let base_r = fighter(1, 0, UnitType::Spearman, 1_000, 30);
    let base_a = arrival(2, 1, UnitType::Spearman, 1_000, 30);
    let base_g = garrison(3, 2, 31, 1_000);
    let run = |r: Fighter, g: Garrison, a: Fighter| {
        clash_with(
            &t,
            &[r],
            &[g],
            &[a],
            Relations::ALL_HOSTILE,
            Occupancy::EMPTY,
        )
        .map(|_| ())
    };
    assert_eq!(run(base_r, base_g, base_a), Ok(()));

    // Troops: at the cap accepted; cap + 1 and u32::MAX refused.
    for (troops, ok) in [
        (MAX_HOST_TROOPS, true),
        (MAX_HOST_TROOPS + 1, false),
        (u32::MAX, false),
    ] {
        let want = |id| {
            if ok {
                Ok(())
            } else {
                Err(ClashError::TroopsAboveCap(id))
            }
        };
        assert_eq!(
            run(Fighter { troops, ..base_r }, base_g, base_a),
            want(1),
            "resident {troops}"
        );
        assert_eq!(
            run(base_r, base_g, Fighter { troops, ..base_a }),
            want(2),
            "arrival {troops}"
        );
        assert_eq!(
            run(base_r, Garrison { troops, ..base_g }, base_a),
            want(3),
            "garrison {troops}"
        );
    }
    // Stamina.
    for (stamina, ok) in [
        (STAMINA_CAP, true),
        (STAMINA_CAP + 1, false),
        (u16::MAX, false),
    ] {
        let want = |id| {
            if ok {
                Ok(())
            } else {
                Err(ClashError::StaminaAboveCap(id))
            }
        };
        assert_eq!(run(Fighter { stamina, ..base_r }, base_g, base_a), want(1));
        assert_eq!(run(base_r, base_g, Fighter { stamina, ..base_a }), want(2));
    }
    // Doctrine multiplier: [BPS_ONE, COMBAT_MAX_BPS].
    for (dealt, ok) in [
        (0, false),
        (BPS_ONE - 1, false),
        (BPS_ONE, true),
        (COMBAT_MAX_BPS, true),
        (COMBAT_MAX_BPS + 1, false),
        (u32::MAX, false),
    ] {
        let want = |id| {
            if ok {
                Ok(())
            } else {
                Err(ClashError::BadMultiplier(id))
            }
        };
        assert_eq!(
            run(
                Fighter {
                    dealt_bps: dealt,
                    ..base_r
                },
                base_g,
                base_a
            ),
            want(1),
            "{dealt}"
        );
        assert_eq!(
            run(
                base_r,
                base_g,
                Fighter {
                    dealt_bps: dealt,
                    ..base_a
                }
            ),
            want(2),
            "{dealt}"
        );
    }
    // Retreat ratio (I-27): ≤ RETREAT_MAX_BPS = 60,000.
    assert_eq!(RETREAT_MAX_BPS, 60_000);
    assert_eq!(RETREAT_CAP, RETREAT_MAX_BPS as u32);
    for (r, ok) in [
        (1, true),
        (RETREAT_MAX_BPS as Bps, true),
        (RETREAT_MAX_BPS as Bps + 1, false),
        (u32::MAX, false),
    ] {
        let want = |id| {
            if ok {
                Ok(())
            } else {
                Err(ClashError::BadRetreat(id))
            }
        };
        assert_eq!(
            run(
                base_r,
                base_g,
                Fighter {
                    retreat_bps: Some(r),
                    ..base_a
                }
            ),
            want(2),
            "{r}"
        );
        assert_eq!(
            run(
                Fighter {
                    retreat_bps: Some(r),
                    ..base_r
                },
                base_g,
                base_a
            ),
            want(1),
            "{r}"
        );
    }
    // Faction ids (CL-06): 0..=5 and NEUTRAL (6); 7 and above refused.
    for (f, ok) in [
        (0u8, true),
        (5, true),
        (NEUTRAL, true),
        (7, false),
        (u8::MAX, false),
    ] {
        let want = |id| {
            if ok {
                Ok(())
            } else {
                Err(ClashError::BadFaction(id))
            }
        };
        assert_eq!(
            run(
                Fighter {
                    faction: f,
                    ..base_r
                },
                base_g,
                base_a
            ),
            want(1),
            "{f}"
        );
        assert_eq!(
            run(
                base_r,
                base_g,
                Fighter {
                    faction: f,
                    ..base_a
                }
            ),
            want(2),
            "{f}"
        );
        assert_eq!(
            run(
                base_r,
                Garrison {
                    faction: f,
                    ..base_g
                },
                base_a
            ),
            want(3),
            "{f}"
        );
    }
}

#[test]
fn faction_ids_are_limited() {
    assert_eq!(NEUTRAL, 6);
    for f in 7..=u8::MAX {
        let mut r = Relations::ALL_HOSTILE;
        r.set_peaceful(0, f, true);
        r.set_peaceful(f, 1, true);
        assert_eq!(r, Relations::ALL_HOSTILE, "faction {f} made peace");
    }
    let mut r = Relations::ALL_HOSTILE;
    r.set_peaceful(NEUTRAL, 0, true);
    assert_eq!(r, Relations::ALL_HOSTILE, "NEUTRAL is never at peace");
    r.set_peaceful(0, 5, true);
    assert!(!r.hostile(0, 5) && !r.hostile(5, 0));
}

/// Golden digest of 300 honest clashes on the **Phase B** kernel
/// (`CLASH_VERSION` 3, W6-B). Phase B changes only the variance stream
/// (one hash per engagement); the same digest came out of W5-A's
/// independent Phase B lab tree (`m1/lab/phaseB-gate/tree-b`, the kernel
/// before the shared sort). Until Phase B the value was the one recorded
/// with the kernel at `d95fa25` (before CL-01, CL-10, I-43 and Phase A),
/// `c2c75bd7f21633451cf0e5a59943568f2c8af635964b925e6ee4268611b554ba`,
/// which this test held through wave 5: the bounds, CL-10, I-43 and
/// Phase A left every honest outcome of this set unchanged.
const PHASE_B_HONEST_300: &str = "4fe17f5e2f47f94a308b47cf0f71d0feecf30fa1c784f8da4c74f2e573567409";

#[test]
fn clash_bounds_do_not_change_honest_outcomes() {
    let mut rng = Rng(0xC1_01);
    let mut acc = [0u8; 32];
    let (mut fought, mut arrived) = (0, 0);
    for _ in 0..300 {
        let h = honest(&mut rng);
        let r = run_honest(&h);
        let o = r.as_ref().expect("an honest clash is valid");
        fought += (o.engagements > 0) as u32;
        arrived += h.arrivals.len();
        fold(&mut acc, &r);
    }
    assert!(fought > 150, "only {fought} honest clashes fought");
    assert!(arrived > 1_000);
    assert_eq!(
        hex(&acc),
        PHASE_B_HONEST_300,
        "honest outcomes moved against the Phase B recording"
    );
}

// ------------------------------------------------------------ CL-14

#[test]
fn damage_product_cannot_overflow() {
    // The bound is the largest stance multiplier times the largest doctrine
    // multiplier `validate` accepts.
    let mut stance_max = 0;
    let all = [
        Posture::Stance(Stance::Hold),
        Posture::Stance(Stance::Assault),
        Posture::Stance(Stance::Flank),
        Posture::Stance(Stance::Brace),
        Posture::Disarray,
    ];
    for a in all {
        for d in all {
            stance_max = stance_max.max(damage_bps(a, d));
        }
    }
    assert_eq!(MAX_STANCE_BPS, stance_max);
    assert_eq!(
        MAX_DAMAGE_PRODUCT_BPS as u128,
        MAX_STANCE_BPS as u128 * COMBAT_MAX_BPS as u128 / BPS_ONE as u128
    );
    // Both intermediate products of `scale` stay inside u64 for any raw
    // damage a u32 can hold, and the host bound of CL-14 holds with room.
    let raw = u32::MAX as u128;
    assert!(raw * MAX_STANCE_BPS as u128 <= u64::MAX as u128);
    assert!(
        raw * MAX_STANCE_BPS as u128 / BPS_ONE as u128 * COMBAT_MAX_BPS as u128 <= u64::MAX as u128
    );
    assert!(MAX_HOST_TROOPS as u128 * MAX_DAMAGE_PRODUCT_BPS as u128 <= u64::MAX as u128);

    // The extreme fighters at every cap, both halves of every engagement:
    // every pairing of stances and Disarray, residents against arrivals and
    // against each other, walls and rough ground. Must not panic (the
    // test profile has overflow checks) and must destroy or keep troops
    // within bounds.
    let t = flat();
    let mut rough = flat();
    rough.terrain = [Terrain::Hills; PROVINCE_TILES];
    for terrain in [&t, &rough] {
        for (x, pa) in all.iter().enumerate() {
            for (y, pd) in all.iter().enumerate() {
                let arrival_posture = match pa {
                    Posture::Disarray => Posture::Stance(Stance::Assault),
                    p => *p,
                };
                let big = |id, f, unit, posture, tile| Fighter {
                    id,
                    faction: f,
                    unit,
                    troops: MAX_HOST_TROOPS,
                    stamina: STAMINA_CAP,
                    tile,
                    posture,
                    retreat_bps: None,
                    dealt_bps: COMBAT_MAX_BPS,
                };
                let residents = [
                    big(1, 0, UnitType::Knight, *pd, 30),
                    big(2, 1, UnitType::Pikeman, *pa, 30),
                    big(3, 2, UnitType::Crossbowman, *pd, 30),
                ];
                let arrivals = [
                    big(4, 3, UnitType::Knight, arrival_posture, 30),
                    big(5, 4, UnitType::Pikeman, arrival_posture, 30),
                    big(6, 5, UnitType::Knight, arrival_posture, 31),
                ];
                let garrisons = [Garrison {
                    id: 7,
                    faction: NEUTRAL,
                    tile: 31,
                    troops: MAX_HOST_TROOPS,
                    walls: (x + y) % 2 == 0,
                    posture: *pd,
                }];
                let o = clash_with(
                    terrain,
                    &residents,
                    &garrisons,
                    &arrivals,
                    Relations::ALL_HOSTILE,
                    Occupancy::EMPTY,
                )
                .expect("valid");
                assert!(o.engagements > 0);
                for f in &o.fighters {
                    assert!(f.troops <= MAX_HOST_TROOPS);
                }
            }
        }
    }
}

#[test]
fn validate_table_refuses_a_product_above_the_bound() {
    assert_eq!(validate_table(&DOCTRINES), Ok(()));
    // Every doctrine of the Season 1 table deals at most COMBAT_MAX_BPS in
    // any posture, arriving or not, so no honest fighter is refused.
    for d in DOCTRINES.iter() {
        for p in [
            Posture::Stance(Stance::Hold),
            Posture::Stance(Stance::Assault),
            Posture::Stance(Stance::Flank),
            Posture::Stance(Stance::Brace),
            Posture::Disarray,
        ] {
            for arrival in [false, true] {
                let m = d.dealt_bps(p, arrival);
                assert!(
                    (BPS_ONE..=COMBAT_MAX_BPS).contains(&m),
                    "{} {p:?} {arrival}: {m}",
                    d.name
                );
            }
        }
    }
    // F's heavy cavalry drilled in Assault: 1.10 × 1.10 = 1.21 > 1.15. Each
    // knob alone passes `Doctrine::validate`; the product does not.
    let mut t = DOCTRINES;
    t[5].drill = Some((Stance::Assault, 11_000));
    assert_eq!(t[5].validate(), Ok(()));
    assert_eq!(validate_table(&t), Err(DoctrineError::DamageProduct));
    // C's drill and arrival bonus at their caps: 1.15 × 1.15 on the bell it
    // arrives in its drilled stance.
    let mut t = DOCTRINES;
    t[2].drill = Some((Stance::Assault, COMBAT_MAX_BPS));
    t[2].arrival_bps = COMBAT_MAX_BPS;
    assert_eq!(validate_table(&t), Err(DoctrineError::DamageProduct));
    // At the bound exactly: accepted.
    let mut t = DOCTRINES;
    t[5].variant_bps = COMBAT_MAX_BPS;
    assert_eq!(validate_table(&t), Ok(()));
}

// ------------------------------------------------------------ CL-10

/// CL-10 (a), failing-first (failed at `d95fa25`): an arrival bounced by
/// the per-faction or the province cap is re-admitted, in mass order, when
/// the hex fair share bounces another host and frees the slot.
#[test]
fn a_bounce_by_fair_share_frees_a_cap_slot() {
    // Faction 0: 3 residents on hex 30, 4 elsewhere (7 of its 8). Faction 1
    // arrives on hex 30 with 3 heavy hosts. Faction 0 sends A1 (heavier,
    // onto hex 30) and A2 (lighter, onto the empty hex 40).
    let mut residents: Vec<Fighter> = (0..3)
        .map(|i| fighter(10 + i, 0, UnitType::Spearman, 5_000, 30))
        .collect();
    residents.extend((0..4).map(|i| fighter(20 + i, 0, UnitType::Spearman, 5_000, 10 + i as u8)));
    let mut arrivals: Vec<Fighter> = (0..3)
        .map(|i| arrival(30 + i, 1, UnitType::Spearman, 6_000, 30))
        .collect();
    arrivals.push(arrival(40, 0, UnitType::Spearman, 4_000, 30)); // A1
    arrivals.push(arrival(41, 0, UnitType::Spearman, 1_000, 40)); // A2
                                                                  // Per-faction cap: A1 takes faction 0's 8th slot, A2 is refused; hex 30
                                                                  // then holds 7 hosts and the fair share (3 per side) bounces A1, the
                                                                  // lightest of faction 0 there. A2 now fits.
    let o = clash(&residents, &[], &arrivals);
    assert_eq!(
        o.fighter(40).unwrap().fate,
        Fate::Bounced,
        "A1 loses the fair share"
    );
    assert_eq!(
        o.fighter(41).unwrap().fate,
        Fate::Stays { tile: 40 },
        "A2 takes the freed slot"
    );

    // Province cap: 47 residents (faction 0 has 7, factions 1..=5 have 8),
    // hex 30 holds 3 of faction 0 and 3 heavy residents of faction 1.
    let mut residents: Vec<Fighter> = (0..3)
        .map(|i| fighter(100 + i, 0, UnitType::Spearman, 5_000, 30))
        .chain((0..3).map(|i| fighter(110 + i, 1, UnitType::Spearman, 6_000, 30)))
        .collect();
    let mut tile = 0u8;
    let mut next_tile = || {
        tile += 1;
        while tile == 30 || tile == 40 {
            tile += 1;
        }
        tile
    };
    for i in 0..4 {
        residents.push(fighter(120 + i, 0, UnitType::Spearman, 5_000, next_tile()));
    }
    for i in 0..5 {
        residents.push(fighter(130 + i, 1, UnitType::Spearman, 5_000, next_tile()));
    }
    for f in 2..6u8 {
        for i in 0..8 {
            let t = next_tile();
            residents.push(fighter(
                200 + 10 * f as u64 + i,
                f,
                UnitType::Spearman,
                5_000,
                t % 61,
            ));
        }
    }
    assert_eq!(residents.len(), PROVINCE_HOST_CAP - 1);
    let arrivals = [
        arrival(300, 0, UnitType::Spearman, 4_000, 30), // A1: the 48th host
        arrival(301, 0, UnitType::Spearman, 1_000, 40), // A2: refused by both caps
    ];
    let o = clash(&residents, &[], &arrivals);
    assert_eq!(o.fighter(300).unwrap().fate, Fate::Bounced);
    assert_eq!(o.fighter(301).unwrap().fate, Fate::Stays { tile: 40 });
}

/// CL-10 (a), the recount never displaces: a re-admitted arrival takes a
/// free slot of its hex or none. Faction 0's slot is freed as in
/// `a_bounce_by_fair_share_frees_a_cap_slot`, but A2's hex is full (six
/// lighter hosts of faction 2), or, on a holding's hex, its three slots for
/// hosts hostile to the owner are taken: A2 stays home and nobody there is
/// bounced.
#[test]
fn the_cap_recount_takes_only_free_hex_slots() {
    let base = || {
        let mut residents: Vec<Fighter> = (0..3)
            .map(|i| fighter(10 + i, 0, UnitType::Spearman, 5_000, 30))
            .collect();
        residents
            .extend((0..4).map(|i| fighter(20 + i, 0, UnitType::Spearman, 5_000, 10 + i as u8)));
        let mut arrivals: Vec<Fighter> = (0..3)
            .map(|i| arrival(30 + i, 1, UnitType::Spearman, 6_000, 30))
            .collect();
        arrivals.push(arrival(40, 0, UnitType::Spearman, 4_000, 30)); // A1
        (residents, arrivals)
    };
    // A full hex: six lighter residents of faction 2.
    let (mut residents, mut arrivals) = base();
    residents.extend((0..6).map(|i| fighter(60 + i, 2, UnitType::Spearman, 500, 50)));
    arrivals.push(arrival(41, 0, UnitType::Spearman, 1_000, 50)); // A2
    let o = clash(&residents, &[], &arrivals);
    assert_eq!(o.fighter(40).unwrap().fate, Fate::Bounced);
    assert_eq!(
        o.fighter(41).unwrap().fate,
        Fate::Bounced,
        "no free slot on hex 50"
    );
    for i in 0..6 {
        assert_ne!(
            o.fighter(60 + i).unwrap().fate,
            Fate::Bounced,
            "nobody is displaced"
        );
    }
    // A holding's hex: faction 3 owns it, three hosts of faction 1 (hostile
    // to it) hold the hostile slots, three are free for the owner's side.
    let (mut residents, mut arrivals) = base();
    residents.extend((0..3).map(|i| fighter(70 + i, 1, UnitType::Spearman, 100, 51)));
    arrivals.push(arrival(41, 0, UnitType::Spearman, 1_000, 51)); // A2, hostile to 3
    let g = [garrison(90, 3, 51, 0)];
    let o = clash(&residents, &g, &arrivals);
    assert_eq!(
        o.fighter(41).unwrap().fate,
        Fate::Bounced,
        "the hostile slots are taken"
    );
    // … while an owner's ally would fit: make 0 and 3 peaceful.
    let mut rel = Relations::ALL_HOSTILE;
    rel.set_peaceful(0, 3, true);
    let o = clash_with(&flat(), &residents, &g, &arrivals, rel, Occupancy::EMPTY).unwrap();
    assert_eq!(
        o.fighter(41).unwrap().fate,
        Fate::Stays { tile: 51 },
        "a free slot on the owner's side"
    );
}

/// CL-10 (b), failing-first (failed at `d95fa25`): units with no attack
/// and no defence (Scout, Settler) never contest a tile, never count in
/// the field-holding ranking and never hold a holding's hex for a siege;
/// when their faction loses a contested tile they withdraw with it.
#[test]
fn scouts_do_not_contest_a_tile() {
    // 1. A hostile scout arrives beside a resident army: nothing to
    //    contest, both stay, nobody fights.
    let o = clash(
        &[fighter(1, 0, UnitType::Spearman, 2_000, 30)],
        &[],
        &[arrival(2, 1, UnitType::Scout, 500, 30)],
    );
    assert_eq!(o.engagements, 0);
    assert_eq!(o.fighter(1).unwrap().fate, Fate::Stays { tile: 30 });
    assert_eq!(
        o.fighter(2).unwrap().fate,
        Fate::Stays { tile: 30 },
        "the scout does not contest"
    );

    // 2. A hostile scout on a holding's hex whose garrison has no troops
    //    left: it stays, but the hex is not held against the owner.
    let o = clash(
        &[],
        &[garrison(9, 0, 22, 0)],
        &[arrival(3, 1, UnitType::Scout, 500, 22)],
    );
    assert_eq!(o.fighter(3).unwrap().fate, Fate::Stays { tile: 22 });
    let g = o.garrisons[0];
    assert!(!g.attackers_hold, "a scout never advances a siege");
    assert_eq!(g.holders, 0);
    // … and the owner's own scout is no defender either.
    let o = clash(
        &[fighter(4, 0, UnitType::Scout, 500, 22)],
        &[garrison(9, 0, 22, 0)],
        &[arrival(5, 1, UnitType::Spearman, 500, 22)],
    );
    assert_eq!(o.fighter(4).unwrap().fate, Fate::Stays { tile: 22 });
    let g = o.garrisons[0];
    assert!(g.attackers_hold && g.holders == 0b10);
    assert!(!g.defender_present, "a scout does not defend a holding");

    // 3. Two hostile scouts share a hex.
    let o = clash(
        &[fighter(6, 0, UnitType::Scout, 500, 31)],
        &[],
        &[arrival(7, 1, UnitType::Scout, 500, 31)],
    );
    assert_eq!(o.fighter(6).unwrap().fate, Fate::Stays { tile: 31 });
    assert_eq!(o.fighter(7).unwrap().fate, Fate::Stays { tile: 31 });

    // 4. A contested hex: faction 0's army beats faction 1's; faction 1's
    //    scout withdraws with its army, faction 2's scout (at peace with 0)
    //    stays. (Had faction 1's army been destroyed, its scout would face
    //    the army alone and stay, as in case 1.)
    let mut rel = Relations::ALL_HOSTILE;
    rel.set_peaceful(0, 2, true);
    let o = clash_with(
        &flat(),
        &[
            fighter(10, 0, UnitType::Spearman, 10_000, 30),
            fighter(12, 2, UnitType::Scout, 500, 30),
        ],
        &[],
        &[
            arrival(11, 1, UnitType::Spearman, 7_000, 30),
            arrival(13, 1, UnitType::Scout, 500, 30),
        ],
        rel,
        Occupancy::EMPTY,
    )
    .unwrap();
    assert_eq!(o.fighter(10).unwrap().fate, Fate::Stays { tile: 30 });
    assert_eq!(
        o.fighter(11).unwrap().fate,
        Fate::Bounced,
        "faction 1 loses the field"
    );
    assert_eq!(
        o.fighter(13).unwrap().fate,
        Fate::Bounced,
        "the loser's scout goes home"
    );
    assert_eq!(
        o.fighter(12).unwrap().fate,
        Fate::Stays { tile: 30 },
        "a friendly scout stays"
    );
}

// ------------------------------------------------------------ I-43

/// I-43, failing-first (the room is new): pending musters count against
/// the province and faction caps, and departed entries against storage, so
/// entries after the resolve never exceed 56 nor 8 per faction once the
/// musters join.
#[test]
fn pending_musters_and_departed_entries_bound_the_stays() {
    // The reviewer's case: 40 residents, 8 pending musters, 8 departed
    // entries awaiting SettleDeparture: storage is full, one arrival bounces.
    let mut residents = Vec::new();
    let mut t = 0u8;
    for f in 0..5u8 {
        for i in 0..8u64 {
            residents.push(fighter(
                100 + 10 * f as u64 + i,
                f,
                UnitType::Spearman,
                1_000,
                t,
            ));
            t += 1;
        }
    }
    let mut occ = Occupancy::EMPTY;
    occ.pending[5] = 8;
    occ.storage_free = (56 - 40 - 8 - 8) as u8;
    let a = [arrival(900, 5, UnitType::Spearman, 1_000, 50)];
    let o = clash_with(&flat(), &residents, &[], &a, Relations::ALL_HOSTILE, occ).unwrap();
    assert_eq!(
        o.fighter(900).unwrap().fate,
        Fate::Bounced,
        "no storage left"
    );
    // With the same room but the musters of another faction, the arrival is
    // refused by storage alone.
    let occ2 = Occupancy {
        pending: [0; 8],
        storage_free: 0,
    };
    let o = clash_with(&flat(), &residents, &[], &a, Relations::ALL_HOSTILE, occ2).unwrap();
    assert_eq!(o.fighter(900).unwrap().fate, Fate::Bounced);
    // Without the room (v1.0 kernel): it would stay.
    let o = clash_with(
        &flat(),
        &residents,
        &[],
        &a,
        Relations::ALL_HOSTILE,
        Occupancy::EMPTY,
    )
    .unwrap();
    assert_eq!(o.fighter(900).unwrap().fate, Fate::Stays { tile: 50 });

    // 32 residents (factions 0..=3), 4 pending musters of faction 4, 16
    // departed entries: storage_free = 56 − 32 − 4 − 16 = 4. Eight arrivals
    // of factions 4 and 5 on empty hexes: the four heaviest stay, in kernel
    // mass order, the rest bounce with no loss.
    let residents: Vec<Fighter> = residents.into_iter().filter(|f| f.faction < 4).collect();
    let mut occ = Occupancy::EMPTY;
    occ.pending[4] = 4;
    occ.storage_free = 4;
    let arrivals: Vec<Fighter> = (0..8u64)
        .map(|i| {
            arrival(
                500 + i,
                4 + (i % 2) as u8,
                UnitType::Spearman,
                1_000 + 100 * i as u32,
                40 + i as u8,
            )
        })
        .collect();
    let o = clash_with(
        &flat(),
        &residents,
        &[],
        &arrivals,
        Relations::ALL_HOSTILE,
        occ,
    )
    .unwrap();
    for a in &arrivals {
        let f = o.fighter(a.id).unwrap();
        if a.id >= 504 {
            assert_eq!(
                f.fate,
                Fate::Stays { tile: a.tile },
                "{} is among the 4 heaviest",
                a.id
            );
        } else {
            assert_eq!(f.fate, Fate::Bounced, "{} finds no storage", a.id);
            assert_eq!(f.troops, a.troops, "a bounce loses nothing");
        }
    }
    // Faction 4 has 6 pending musters: at most 2 of its 4 arrivals may stay
    // (8 per faction once the musters join).
    let mut occ = Occupancy::EMPTY;
    occ.pending[4] = 6;
    let arrivals: Vec<Fighter> = (0..4u64)
        .map(|i| {
            arrival(
                600 + i,
                4,
                UnitType::Spearman,
                1_000 + 100 * i as u32,
                40 + i as u8,
            )
        })
        .collect();
    let o = clash_with(
        &flat(),
        &residents,
        &[],
        &arrivals,
        Relations::ALL_HOSTILE,
        occ,
    )
    .unwrap();
    let stays = arrivals
        .iter()
        .filter(|a| matches!(o.fighter(a.id).unwrap().fate, Fate::Stays { .. }))
        .count();
    assert_eq!(stays + occ.pending[4] as usize, FACTION_RESIDENT_CAP);
    // Pending musters also count against the province's 48: 32 residents,
    // 8 NEUTRAL camps and 8 pending musters of faction 5 leave faction 4 no
    // room although it has none of its own.
    let mut residents = residents;
    for i in 0..8u64 {
        residents.push(fighter(
            700 + i,
            NEUTRAL,
            UnitType::Spearman,
            100,
            50 + i as u8,
        ));
    }
    let mut occ = Occupancy::EMPTY;
    occ.pending[5] = 8;
    assert_eq!(residents.len() + 8, PROVINCE_HOST_CAP);
    let o = clash_with(
        &flat(),
        &residents,
        &[],
        &arrivals,
        Relations::ALL_HOSTILE,
        occ,
    )
    .unwrap();
    assert!(arrivals
        .iter()
        .all(|a| o.fighter(a.id).unwrap().fate == Fate::Bounced));
    // Without the musters the same arrivals stay.
    let o = clash_with(
        &flat(),
        &residents,
        &[],
        &arrivals,
        Relations::ALL_HOSTILE,
        Occupancy::EMPTY,
    )
    .unwrap();
    assert!(arrivals
        .iter()
        .all(|a| matches!(o.fighter(a.id).unwrap().fate, Fate::Stays { .. })));
}

// ------------------------------------------------------------ integ-W1 review

/// §7: the clash's private faction rule equals `geometry::valid_faction`
/// for every `u8`, through `clash::validate` (hosts, arrivals and
/// garrisons accept NEUTRAL) and `Relations::set_peaceful` (only the six
/// player factions can make peace).
#[test]
fn clash_faction_rule_equals_geometry() {
    use permutation_rules::frontier::geometry::valid_faction;
    let t = flat();
    for f in 0..=u8::MAX {
        let r = resolve_one(&t, &[fighter(1, f, UnitType::Spearman, 100, 30)], &[], &[]);
        assert_eq!(
            r.is_ok(),
            valid_faction(f, true),
            "resident faction {f}: {r:?}"
        );
        if !valid_faction(f, true) {
            assert_eq!(r, Err(ClashError::BadFaction(1)));
        }
        let a = resolve_one(&t, &[], &[], &[arrival(2, f, UnitType::Spearman, 100, 30)]);
        assert_eq!(a.is_ok(), valid_faction(f, true), "arrival faction {f}");
        let g = resolve_one(&t, &[], &[garrison(3, f, 31, 100)], &[]);
        assert_eq!(g.is_ok(), valid_faction(f, true), "garrison faction {f}");
        for other in 0..6u8 {
            let mut rel = Relations::ALL_HOSTILE;
            rel.set_peaceful(other, f, true);
            let made = rel != Relations::ALL_HOSTILE;
            assert_eq!(
                made,
                other != f && valid_faction(f, false),
                "peace {other} {f}"
            );
        }
    }
}

fn resolve_one(
    t: &ProvinceTerrain,
    residents: &[Fighter],
    garrisons: &[Garrison],
    arrivals: &[Fighter],
) -> Result<ClashOutcome, ClashError> {
    clash_with(
        t,
        residents,
        garrisons,
        arrivals,
        Relations::ALL_HOSTILE,
        Occupancy::EMPTY,
    )
}

/// CL-01 review (integ-W1): a garrison can never be built above the cap
/// the clash enforces, so a Garrison top-up cannot wedge a province. A
/// garrison exactly at `MAX_HOST_TROOPS` resolves against besiegers and
/// its quiet bell is recognised; the reviewer's sequence (`new(MAX)`, then
/// `+1,000,000` milli, then `settle`) is refused and leaves the garrison
/// at the cap.
#[test]
fn a_garrison_at_the_cap_resolves_and_cannot_pass_it() {
    use permutation_rules::frontier::clash::is_quiet;
    use permutation_rules::frontier::host::{GarrisonState, HostError};
    let cap = MAX_HOST_TROOPS / 1000;
    let t = flat();
    let g = [garrison(3, 2, 31, cap)];
    let besiegers = [fighter(1, 0, UnitType::Spearman, cap, 31)];
    let o = resolve_one(&t, &besiegers, &g, &[]).expect("a garrison at the cap resolves");
    assert!(o.engagements > 0);
    let quiet = ClashInput {
        province: P,
        bell: 11,
        seed: sha256(&[b"bounds"]),
        terrain: &t,
        residents: &[],
        garrisons: &g,
        arrivals: &[],
        relations: Relations::ALL_HOSTILE,
        occupancy: Occupancy::EMPTY,
    };
    assert_eq!(is_quiet(&frontier_ruleset(), &quiet), Ok(true));
    assert_eq!(
        resolve_one(&t, &[], &[garrison(3, 2, 31, cap + 1)], &[]).err(),
        Some(ClashError::TroopsAboveCap(3)),
        "the clash still refuses a garrison above the cap"
    );

    let mut gs = GarrisonState::new(MAX_HOST_TROOPS);
    assert_eq!(gs.room(), 0);
    assert_eq!(gs.change(1, 1_000_000, 0), Err(HostError::TooLarge));
    assert_eq!(gs.change(1, 1, 0), Err(HostError::TooLarge));
    gs.change(1, -5_000, 0).unwrap();
    assert_eq!(gs.room(), 5_000);
    assert_eq!(gs.change(1, 5_001, 0), Err(HostError::TooLarge));
    gs.change(1, 5_000, 0).unwrap();
    gs.settle(2);
    assert_eq!(gs.at(2), Ok(MAX_HOST_TROOPS));
    // Clamped even if built from an out-of-range value.
    assert_eq!(GarrisonState::new(u32::MAX).at(0), Ok(MAX_HOST_TROOPS));
    // Two bells pending: the projection counts both.
    let mut gs = GarrisonState::new(MAX_HOST_TROOPS - 3_000);
    gs.change(5, 2_000, 4).unwrap();
    gs.change(6, 1_000, 5).unwrap();
    assert_eq!(gs.change(6, 1, 5), Err(HostError::TooLarge));
    let mut t = GarrisonState::new(MAX_HOST_TROOPS - 3_000);
    t.change(5, 2_000, 4).unwrap();
    assert_eq!(t.change(6, 1_001, 5), Err(HostError::TooLarge));
}
