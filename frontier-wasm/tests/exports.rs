//! The exports on the host (M1 contract §9.5; W2-E: "until the wasm32
//! target is approved the exports are tested on the host"). Every call goes
//! through the frame protocol (`frontier_wasm::call`), so what is tested is
//! what the browser calls. Cross-checks: the ruleset hash against
//! `frontier-abi/vectors/presets.json`, the seal exports against
//! `permutation-rules/vectors/seal-vectors-v1.json`, the clock exports
//! against `clock-vectors-v1.json`, the rest against direct kernel calls.
//!
//! `wasm_vectors_are_fresh` writes (with `PSF_WRITE_VECTORS=1`) or checks
//! `frontier-wasm/vectors/wasm-vectors.json`: one call per export with its
//! arguments as JSON, the borsh input and the framed answer. The web test
//! `web-frontier-wasm.test.mjs` encodes the same arguments with `wasm.mjs`
//! (bytes must match) and, once `frontier.wasm` exists, runs every call
//! through it.

mod json;

use borsh::BorshDeserialize;
use frontier_wasm::api::*;
use frontier_wasm::{call, Answer, BAD_INPUT, OK, REFUSED};
use json::Json;
use permutation_rules::frontier::clash::{self, Fighter, Garrison, Occupancy, Relations};
use permutation_rules::frontier::geometry::{self, ProvinceCoord};
use permutation_rules::frontier::holding::Accrual;
use permutation_rules::frontier::stance::{Posture, Stance};
use permutation_rules::frontier::{beacon, seal, terrain, travel};
use permutation_rules::hex::Hex;
use permutation_rules::units::UnitType;
use std::path::PathBuf;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn read_json(rel: &str) -> Json {
    let p = root().join(rel);
    let s = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    json::parse(&s).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

fn to_hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn ok_of<T: BorshDeserialize>(a: Answer) -> T {
    assert_eq!(
        a.status,
        OK,
        "status {} payload {:?}",
        a.status,
        String::from_utf8_lossy(&a.payload)
    );
    T::try_from_slice(&a.payload).expect("answer decodes")
}

fn refusal(a: Answer) -> Refusal {
    assert_eq!(a.status, REFUSED);
    Refusal::try_from_slice(&a.payload).expect("refusal decodes")
}

fn enc<T: borsh::BorshSerialize>(v: &T) -> Vec<u8> {
    borsh::to_vec(v).expect("encode")
}

type Export = unsafe extern "C" fn(*const u8, usize) -> *mut u8;

fn go(f: Export, args: &impl borsh::BorshSerialize) -> Answer {
    call(f, &enc(args))
}

// ------------------------------------------------------------------ protocol

#[test]
fn every_contract_export_exists() {
    for name in [
        "ruleset_hash",
        "province_of",
        "province_centre",
        "ring_of",
        "wedge_of",
        "region_of",
        "generate_province",
        "plan_path",
        "path_cost",
        "earliest_arrival_bell",
        "check_arrival_bell",
        "bell_at",
        "bell_start",
        "tlock_round",
        "seed_round",
        "plaintext_pack",
        "plaintext_unpack",
        "plaintext_validate",
        "commit",
        "salt_of",
        "body_xor",
        "resolve_clash",
        "resolve_from_inputs",
        "reachable",
        "accrual_at",
    ] {
        assert!(
            frontier_wasm::EXPORTS.contains(&name),
            "§9.5 export {name} missing"
        );
    }
    assert_eq!(
        ok_of::<u32>(call(frontier_wasm::abi_version, &[])),
        frontier_wasm::ABI_VERSION
    );
}

#[test]
fn bad_input_is_a_frame_not_a_panic() {
    // Too short, too long (trailing bytes) and empty.
    for input in [&[1u8, 2, 3][..], &[0u8; 9][..], &[][..]] {
        let a = call(frontier_wasm::ring_of, input);
        assert_eq!(a.status, BAD_INPUT, "{input:?}");
        assert!(a.payload.is_ascii());
    }
    let a = call(frontier_wasm::resolve_from_inputs, &[]);
    assert_eq!(
        a.status, BAD_INPUT,
        "resolve_from_inputs is implemented (W6-D)"
    );
    // alloc/free of zero bytes round-trips.
    let p = frontier_wasm::alloc(0);
    unsafe { frontier_wasm::free(p, 0) };
}

#[test]
fn ruleset_hash_is_the_abi_preset() {
    let presets = read_json("frontier-abi/vectors/presets.json");
    let want = hex(presets.get("ruleset_hash").str());
    let got: [u8; 32] = ok_of(call(frontier_wasm::ruleset_hash, &[]));
    assert_eq!(got.to_vec(), want);
    assert_eq!(got, permutation_rules::frontier::ruleset_hash());
}

// ------------------------------------------------------------------ geometry and terrain

#[test]
fn geometry_exports_equal_the_kernel() {
    let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    for _ in 0..500 {
        let q = (next() % 400) as i32 - 200;
        let r = (next() % 400) as i32 - 200;
        let (p, idx): (ProvinceCoord, u8) = ok_of(go(frontier_wasm::province_of, &Tile { q, r }));
        assert_eq!((p, idx), geometry::locate(Hex::new(q, r)));
        let pq = Pq { p: p.p, q: p.q };
        assert_eq!(
            ok_of::<Hex>(go(frontier_wasm::province_centre, &pq)),
            p.centre()
        );
        assert_eq!(ok_of::<u32>(go(frontier_wasm::ring_of, &pq)), p.ring());
        assert_eq!(
            ok_of::<Option<u8>>(go(frontier_wasm::wedge_of, &pq)),
            p.wedge()
        );
        assert_eq!(
            ok_of::<u8>(go(frontier_wasm::region_of, &pq)),
            geometry::region_of(p)
        );
    }
    assert_eq!(
        ok_of::<Option<u8>>(go(frontier_wasm::wedge_of, &Pq { p: 0, q: 0 })),
        None
    );
}

fn seed_of(ring: u32) -> [u8; 32] {
    let mut s = [0u8; 32];
    s[..4].copy_from_slice(&ring.to_le_bytes());
    s[4..8].copy_from_slice(b"PSFW");
    s
}

#[test]
fn generate_province_is_the_kernels() {
    for (p, q) in [(0, 0), (1, 0), (2, -1), (-3, 3), (5, -2)] {
        let pc = ProvinceCoord::new(p, q);
        let seed = seed_of(pc.ring());
        let g: GeneratedProvince = ok_of(go(
            frontier_wasm::generate_province,
            &GenerateArgs {
                ring_seed: seed,
                p,
                q,
            },
        ));
        assert_eq!(g.terrain, terrain::generate_province(&seed, pc));
        assert_eq!(g.centre, pc.centre());
        assert_eq!(
            (g.ring, g.wedge, g.region),
            (pc.ring(), pc.wedge(), geometry::region_of(pc))
        );
        for i in 0..61u8 {
            assert_eq!(g.passable_mask >> i & 1 == 1, g.terrain.passable(i));
        }
        assert_eq!(g.passable_mask >> 61, 0);
    }
    let r = refusal(go(
        frontier_wasm::generate_province,
        &GenerateArgs {
            ring_seed: [0; 32],
            p: 1_000,
            q: 0,
        },
    ));
    assert_eq!(r.code, 1, "a ring past R_MAX_HARD is refused");
}

// ------------------------------------------------------------------ paths

fn seeds(rings: u32) -> Vec<RingSeed> {
    (0..=rings)
        .map(|ring| RingSeed {
            ring,
            seed: seed_of(ring),
        })
        .collect()
}

/// The first passable tile of a province (its absolute hex).
fn passable_tile(p: ProvinceCoord) -> Hex {
    let t = terrain::generate_province(&seed_of(p.ring()), p);
    let c = p.centre();
    (0..61u8)
        .filter(|&i| t.passable(i))
        .map(|i| {
            let o = geometry::tile_offset(i).expect("tile");
            Hex::new(c.q + o.q, c.r + o.r)
        })
        .next()
        .expect("a passable tile")
}

#[test]
fn plan_path_gives_a_path_the_kernel_accepts() {
    let mut planned = 0;
    for (a, b) in [
        ((2, 0), (2, 0)),
        ((2, 0), (3, -1)),
        ((2, -1), (2, 0)),
        ((3, 0), (2, 1)),
        ((0, 2), (1, 2)),
    ] {
        let pa = ProvinceCoord::new(a.0, a.1);
        let pb = ProvinceCoord::new(b.0, b.1);
        let start = passable_tile(pa);
        let t = terrain::generate_province(&seed_of(pb.ring()), pb);
        let c = pb.centre();
        // The passable tile of `pb` farthest from `start` but within 32 hexes.
        let dest = (0..61u8)
            .filter(|&i| t.passable(i))
            .map(|i| {
                let o = geometry::tile_offset(i).expect("tile");
                Hex::new(c.q + o.q, c.r + o.r)
            })
            .filter(|h| *h != start && h.distance(start) <= 20)
            .max_by_key(|h| (h.distance(start), h.q, h.r))
            .expect("a destination");
        let args = PlanArgs {
            start,
            dest,
            unit: UnitType::Spearman,
            blocked: vec![],
            seeds: seeds(4),
        };
        let plan: Option<PathPlan> = ok_of(go(frontier_wasm::plan_path, &args));
        let Some(plan) = plan else { continue };
        planned += 1;
        assert!(plan.hexes as usize <= travel::MAX_PATH_STEPS);
        assert!(plan.provinces.len() <= travel::MAX_PATH_PROVINCES);
        assert_eq!(
            seal::encode_path(&plan.dirs),
            Some((plan.path_len, plan.path))
        );
        let c: PathCostOut = ok_of(go(
            frontier_wasm::path_cost,
            &PathCostArgs {
                start,
                dirs: plan.dirs.clone(),
                unit: UnitType::Spearman,
                seeds: seeds(4),
            },
        ));
        assert_eq!(
            (c.secs, c.hexes, &c.provinces, c.end),
            (plan.secs, plan.hexes, &plan.provinces, dest)
        );
        // A cavalry plan is never slower.
        let fast: Option<PathPlan> = ok_of(go(
            frontier_wasm::plan_path,
            &PlanArgs {
                unit: UnitType::Horseman,
                ..args.clone()
            },
        ));
        assert!(fast.expect("horse path").secs <= plan.secs);
        // Blocking the first step forces another path or none, never the same.
        let first = {
            let (dq, dr) = permutation_rules::hex::DIRECTIONS[plan.dirs[0] as usize];
            Hex::new(start.q + dq, start.r + dr)
        };
        if first != dest {
            let other: Option<PathPlan> = ok_of(go(
                frontier_wasm::plan_path,
                &PlanArgs {
                    blocked: vec![first],
                    ..args.clone()
                },
            ));
            if let Some(o) = other {
                assert_ne!(o.dirs[0], plan.dirs[0]);
            }
        }
        // Unknown land (no seed for the destination's ring) is never entered.
        let none: Option<PathPlan> = ok_of(go(
            frontier_wasm::plan_path,
            &PlanArgs {
                seeds: seeds(4)
                    .into_iter()
                    .filter(|s| s.ring != pb.ring())
                    .collect(),
                ..args.clone()
            },
        ));
        assert!(none.is_none());
    }
    assert!(planned >= 4, "only {planned} of 5 pairs planned");
    // Too far and same-hex: None.
    let far = PlanArgs {
        start: Hex::new(0, 0),
        dest: Hex::new(40, 0),
        unit: UnitType::Scout,
        blocked: vec![],
        seeds: seeds(4),
    };
    assert!(ok_of::<Option<PathPlan>>(go(frontier_wasm::plan_path, &far)).is_none());
}

#[test]
fn path_cost_refusals_are_the_kernels() {
    let start = passable_tile(ProvinceCoord::new(2, 0));
    let r = refusal(go(
        frontier_wasm::path_cost,
        &PathCostArgs {
            start,
            dirs: vec![],
            unit: UnitType::Spearman,
            seeds: seeds(3),
        },
    ));
    assert_eq!((r.code, r.arg), (1, 0), "EmptyPath");
    let r = refusal(go(
        frontier_wasm::path_cost,
        &PathCostArgs {
            start,
            dirs: vec![0; 33],
            unit: UnitType::Spearman,
            seeds: seeds(3),
        },
    ));
    assert_eq!(r.code, 2, "TooLong");
    let r = refusal(go(
        frontier_wasm::path_cost,
        &PathCostArgs {
            start,
            dirs: vec![0, 7],
            unit: UnitType::Spearman,
            seeds: seeds(3),
        },
    ));
    assert_eq!((r.code, r.arg), (9, 1), "a direction ≥ 6");
    // Unknown land prices as impassable.
    let r = refusal(go(
        frontier_wasm::path_cost,
        &PathCostArgs {
            start,
            dirs: vec![0],
            unit: UnitType::Spearman,
            seeds: vec![],
        },
    ));
    assert_eq!((r.code, r.arg), (4, 0), "Impassable");
}

// ------------------------------------------------------------------ clock

#[test]
fn clock_exports_follow_the_clock_vectors() {
    let v = read_json("permutation-rules/vectors/clock-vectors-v1.json");
    let drand = Drand {
        genesis: v.get("clock").get("genesis").i64(),
        period: v.get("clock").get("period").i64() as u32,
    };
    for b in v.get("bells").arr() {
        let genesis_ts = b.get("genesis_ts").i64();
        let bell = b.get("bell").i64() as u32;
        let round: u64 = ok_of(go(
            frontier_wasm::tlock_round,
            &TlockArgs {
                drand,
                genesis_ts,
                bell,
            },
        ));
        assert_eq!(round, b.get("tlock_round").i64() as u64);
        let start: i64 = ok_of(go(
            frontier_wasm::bell_start,
            &BellStartArgs { genesis_ts, bell },
        ));
        assert_eq!(start, b.get("bell_start").i64());
        if bell < u32::MAX {
            let at: Option<u32> = ok_of(go(
                frontier_wasm::bell_at,
                &BellAtArgs {
                    genesis_ts,
                    t: start,
                },
            ));
            assert_eq!(at, Some(bell));
        }
    }
    for s in v.get("seed_rounds").arr() {
        let args = SeedRoundArgs {
            drand,
            a: s.get("anchor_ts").i64(),
            window: s.get("window").i64() as u32,
            margin: s.get("margin").i64() as u32,
        };
        assert_eq!(
            ok_of::<u64>(go(frontier_wasm::seed_round, &args)),
            s.get("seed_round").i64() as u64
        );
    }
    let before: Option<u32> = ok_of(go(
        frontier_wasm::bell_at,
        &BellAtArgs {
            genesis_ts: 1_000,
            t: 999,
        },
    ));
    assert_eq!(before, beacon::bell_at(1_000, 999));
}

#[test]
fn arrival_bells_are_the_travel_kernels() {
    let g = 1_800_000_000;
    for (depart, secs) in [(g, 0), (g + 599, 60), (g + 6_000, 4_000), (g + 42, 30_000)] {
        let e: u32 = ok_of(go(
            frontier_wasm::earliest_arrival_bell,
            &ArrivalArgs {
                genesis_ts: g,
                depart_ts: depart,
                secs,
            },
        ));
        assert_eq!(e, travel::earliest_arrival_bell(g, depart, secs));
        assert_eq!(
            ok_of::<()>(go(
                frontier_wasm::check_arrival_bell,
                &CheckArrivalArgs {
                    genesis_ts: g,
                    depart_ts: depart,
                    secs,
                    chosen: e
                }
            )),
            ()
        );
        let r = refusal(go(
            frontier_wasm::check_arrival_bell,
            &CheckArrivalArgs {
                genesis_ts: g,
                depart_ts: depart,
                secs,
                chosen: e - 1,
            },
        ));
        assert_eq!(
            (r.code, r.arg),
            (7, e),
            "TooEarly carries the earliest bell"
        );
        let latest = travel::bell_at(g, depart) + 72;
        let r = refusal(go(
            frontier_wasm::check_arrival_bell,
            &CheckArrivalArgs {
                genesis_ts: g,
                depart_ts: depart,
                secs,
                chosen: latest + 1,
            },
        ));
        assert_eq!(
            (r.code, r.arg),
            (8, latest),
            "TooLate carries the latest bell"
        );
    }
}

#[test]
fn reachable_is_optimistic_and_bounded() {
    let g = 1_800_000_000;
    let depart = g + 600 * 10;
    let o = Hex::new(0, 0);
    let yes = ReachArgs {
        origin: o,
        dest: Hex::new(10, 0),
        genesis_ts: g,
        depart_ts: depart,
        target_bell: 12,
        unit: UnitType::Spearman,
    };
    assert!(ok_of::<bool>(go(frontier_wasm::reachable, &yes)));
    assert!(
        !ok_of::<bool>(go(
            frontier_wasm::reachable,
            &ReachArgs {
                target_bell: 11,
                ..yes
            }
        )),
        "never before depart + 2"
    );
    assert!(
        !ok_of::<bool>(go(
            frontier_wasm::reachable,
            &ReachArgs {
                dest: Hex::new(33, 0),
                target_bell: 30,
                ..yes
            }
        )),
        "33 hexes"
    );
    assert!(
        !ok_of::<bool>(go(
            frontier_wasm::reachable,
            &ReachArgs {
                target_bell: 83,
                ..yes
            }
        )),
        "past the 72-bell lead"
    );
    // 30 flat hexes at 120 s = 3,600 s = 6 bells.
    let far = ReachArgs {
        dest: Hex::new(30, 0),
        target_bell: 16,
        ..yes
    };
    assert!(ok_of::<bool>(go(frontier_wasm::reachable, &far)));
    assert!(!ok_of::<bool>(go(
        frontier_wasm::reachable,
        &ReachArgs {
            target_bell: 15,
            ..far
        }
    )));
}

// ------------------------------------------------------------------ seal

fn fields_of(raw: &[u8; seal::PLAIN_LEN]) -> PlainFields {
    ok_of(call(frontier_wasm::plaintext_unpack, raw))
}

#[test]
fn seal_exports_match_the_seal_vectors() {
    let v = read_json("permutation-rules/vectors/seal-vectors-v1.json");
    let cases = v.get("cases").arr();
    assert!(cases.len() >= 24);
    for c in cases {
        let name = c.get("name").str();
        let plain: [u8; 37] = hex(c.get("plain").str()).try_into().expect("37 B");
        let k: [u8; 16] = hex(c.get("k").str()).try_into().expect("16 B");
        let salt: [u8; 32] = hex(c.get("salt").str()).try_into().expect("32 B");
        let sealb: [u8; 165] = hex(c.get("seal").str()).try_into().expect("165 B");
        let f = fields_of(&plain);
        assert_eq!(
            ok_of::<[u8; 37]>(go(frontier_wasm::plaintext_pack, &f)),
            plain,
            "{name}: pack(unpack)"
        );
        assert_eq!(
            ok_of::<[u8; 32]>(go(frontier_wasm::salt_of, &KArgs { k })),
            salt,
            "{name}: salt"
        );
        let commit: [u8; 32] = ok_of(go(frontier_wasm::commit, &CommitArgs { plain, salt }));
        let expect = c.get("expect").str();
        if expect != "commit_mismatch" {
            assert_eq!(to_hex(&commit), c.get("commit").str(), "{name}: commit");
        }
        let ct: [u8; 32] = ok_of(call(frontier_wasm::ct_hash, &sealb));
        assert_eq!(to_hex(&ct), c.get("ct_hash").str(), "{name}: ct_hash");
        let committed: [u8; 32] = hex(c.get("commit").str()).try_into().expect("32 B");
        let root: [u8; 32] = ok_of(go(
            frontier_wasm::seal_root,
            &RootArgs {
                commit: committed,
                ct_hash: ct,
            },
        ));
        assert_eq!(to_hex(&root), c.get("seal_root").str(), "{name}: seal_root");
        if matches!(expect, "valid" | "bad_plaintext") {
            let body: [u8; 37] = ok_of(go(frontier_wasm::body_xor, &BodyArgs { k, plain }));
            assert_eq!(body[..], sealb[128..], "{name}: body");
        }
        let host_id: u64 = c.get("host_id").str().parse().expect("host id");
        let arrive = c.get("arrive_bell").i64() as u32;
        let a = go(
            frontier_wasm::plaintext_validate,
            &ValidateArgs {
                plain,
                host_id,
                arrive_bell: arrive,
            },
        );
        let kernel = seal::validate(&seal::unpack(&plain), host_id, arrive);
        match kernel {
            Ok(()) => {
                assert_eq!(a.status, OK, "{name}");
                assert_eq!(c.get("validate").str(), "ok", "{name}");
            }
            Err(e) => {
                assert_eq!(refusal(a).code, plain_refusal(e), "{name}");
                assert_ne!(c.get("validate").str(), "ok", "{name}");
            }
        }
    }
}

// ------------------------------------------------------------------ clash and accruals

fn fighter(id: u64, faction: u8, troops: u32, tile: u8, arrival: bool) -> Fighter {
    Fighter {
        id,
        faction,
        unit: UnitType::Spearman,
        troops: troops * 1_000,
        stamina: 100,
        tile,
        posture: Posture::Stance(if arrival {
            Stance::Assault
        } else {
            Stance::Hold
        }),
        retreat_bps: None,
        dealt_bps: 10_000,
    }
}

pub fn clash_args() -> ClashArgs {
    let province = ProvinceCoord::new(2, 0);
    let terrain = terrain::generate_province(&seed_of(2), province);
    let tile = (0..61u8)
        .find(|&i| terrain.passable(i) && !terrain.is_site(i))
        .expect("tile");
    let site = terrain.sites[0];
    ClashArgs {
        province,
        bell: 40,
        seed: [7; 32],
        terrain,
        residents: vec![fighter(11, 0, 800, tile, false)],
        garrisons: vec![Garrison {
            id: 90,
            faction: 0,
            tile: site,
            troops: 300_000,
            walls: false,
            posture: Posture::Stance(Stance::Brace),
        }],
        arrivals: vec![
            fighter(21, 1, 1_200, tile, true),
            fighter(22, 2, 500, site, true),
        ],
        relations: Relations::ALL_HOSTILE,
        occupancy: Occupancy::EMPTY,
    }
}

#[test]
fn resolve_clash_is_the_kernels_with_its_digest() {
    let a = clash_args();
    let out: ClashOut = ok_of(go(frontier_wasm::resolve_clash, &a));
    let inp = clash::ClashInput {
        province: a.province,
        bell: a.bell,
        seed: a.seed,
        terrain: &a.terrain,
        residents: &a.residents,
        garrisons: &a.garrisons,
        arrivals: &a.arrivals,
        relations: a.relations,
        occupancy: a.occupancy,
    };
    let direct = clash::resolve_clash(&clash::frontier_ruleset(), &inp).expect("resolves");
    assert_eq!(out.outcome, direct);
    assert_eq!(out.digest, direct.digest());
    let mut bad = clash_args();
    bad.arrivals[0].faction = 9;
    let r = refusal(go(frontier_wasm::resolve_clash, &bad));
    assert_eq!((r.code, r.arg), (5, 21), "BadFaction(id)");
}

#[test]
fn accrual_at_is_value_at() {
    let acc = Accrual::new(5_000, 3_600_000, 50_000, 1_000);
    for t in [0, 1_000, 1_001, 4_600, 100_000] {
        let v: i64 = ok_of(go(
            frontier_wasm::accrual_at,
            &AccrualArgs { accrual: acc, t },
        ));
        assert_eq!(v, acc.value_at(t));
    }
}

// ------------------------------------------------------------------ the shared vector file

/// One recorded call: export, arguments (JSON, the web encoder's input),
/// borsh input, framed answer.
struct Rec {
    export: &'static str,
    args: String,
    input: Vec<u8>,
}

fn rec(export: &'static str, args: String, input: Vec<u8>) -> Rec {
    Rec {
        export,
        args,
        input,
    }
}

fn export_fn(name: &str) -> Export {
    match name {
        "abi_version" => frontier_wasm::abi_version,
        "ruleset_hash" => frontier_wasm::ruleset_hash,
        "province_of" => frontier_wasm::province_of,
        "province_centre" => frontier_wasm::province_centre,
        "ring_of" => frontier_wasm::ring_of,
        "wedge_of" => frontier_wasm::wedge_of,
        "region_of" => frontier_wasm::region_of,
        "generate_province" => frontier_wasm::generate_province,
        "plan_path" => frontier_wasm::plan_path,
        "path_cost" => frontier_wasm::path_cost,
        "earliest_arrival_bell" => frontier_wasm::earliest_arrival_bell,
        "check_arrival_bell" => frontier_wasm::check_arrival_bell,
        "bell_at" => frontier_wasm::bell_at,
        "bell_start" => frontier_wasm::bell_start,
        "tlock_round" => frontier_wasm::tlock_round,
        "seed_round" => frontier_wasm::seed_round,
        "plaintext_pack" => frontier_wasm::plaintext_pack,
        "plaintext_unpack" => frontier_wasm::plaintext_unpack,
        "plaintext_validate" => frontier_wasm::plaintext_validate,
        "commit" => frontier_wasm::commit,
        "salt_of" => frontier_wasm::salt_of,
        "body_xor" => frontier_wasm::body_xor,
        "seal_root" => frontier_wasm::seal_root,
        "ct_hash" => frontier_wasm::ct_hash,
        "resolve_clash" => frontier_wasm::resolve_clash,
        "resolve_from_inputs" => frontier_wasm::resolve_from_inputs,
        "reachable" => frontier_wasm::reachable,
        "accrual_at" => frontier_wasm::accrual_at,
        other => panic!("no export {other}"),
    }
}

fn records() -> Vec<Rec> {
    let seal = read_json("permutation-rules/vectors/seal-vectors-v1.json");
    let c0 = &seal.get("cases").arr()[0];
    let plain: [u8; 37] = hex(c0.get("plain").str()).try_into().expect("37");
    let k: [u8; 16] = hex(c0.get("k").str()).try_into().expect("16");
    let salt: [u8; 32] = hex(c0.get("salt").str()).try_into().expect("32");
    let host_id: u64 = c0.get("host_id").str().parse().expect("id");
    let arrive = c0.get("arrive_bell").i64() as u32;
    let f = fields_of(&plain);
    let drand = Drand {
        genesis: 1_692_803_367,
        period: 3,
    };
    let g = 1_790_384_775i64;
    let s2 = seed_of(2);
    let start = passable_tile(ProvinceCoord::new(2, 0));
    let tile_json = |h: Hex| format!("{{\"q\":{},\"r\":{}}}", h.q, h.r);
    let seeds_json = |s: &[RingSeed]| {
        let parts: Vec<String> = s
            .iter()
            .map(|x| format!("{{\"ring\":{},\"seed\":\"{}\"}}", x.ring, to_hex(&x.seed)))
            .collect();
        format!("[{}]", parts.join(","))
    };
    let seeds3 = seeds(3);
    let dirs_plan: Option<PathPlan> = ok_of(go(
        frontier_wasm::plan_path,
        &PlanArgs {
            start,
            dest: Hex::new(start.q + 3, start.r - 1),
            unit: UnitType::Spearman,
            blocked: vec![],
            seeds: seeds3.clone(),
        },
    ));
    let dirs = dirs_plan.map(|p| p.dirs).unwrap_or_else(|| vec![0]);
    let dest = Hex::new(start.q + 3, start.r - 1);
    let acc = Accrual::new(5_000, 3_600_000, 50_000, 1_000);
    let mut sealb = [0u8; 165];
    sealb.copy_from_slice(&hex(c0.get("seal").str()));
    let ct = seal::ct_hash(&sealb);
    let fields_json = format!(
        "{{\"version\":{},\"host_id\":\"{}\",\"arrive_bell\":{},\"dest_p\":{},\"dest_q\":{},\"dest_tile\":{},\"stance\":{},\"retreat_bps\":{},\"path_len\":{},\"path\":\"{}\",\"reserved\":\"{}\"}}",
        f.version, f.host_id, f.arrive_bell, f.dest_p, f.dest_q, f.dest_tile, f.stance, f.retreat_bps, f.path_len, to_hex(&f.path), to_hex(&f.reserved)
    );
    vec![
        rec("abi_version", "{}".into(), vec![]),
        rec("ruleset_hash", "{}".into(), vec![]),
        rec("province_of", tile_json(Hex::new(17, -9)), enc(&Tile { q: 17, r: -9 })),
        rec("province_centre", "{\"p\":2,\"q\":-1}".into(), enc(&Pq { p: 2, q: -1 })),
        rec("ring_of", "{\"p\":-3,\"q\":1}".into(), enc(&Pq { p: -3, q: 1 })),
        rec("wedge_of", "{\"p\":2,\"q\":-1}".into(), enc(&Pq { p: 2, q: -1 })),
        rec("wedge_of", "{\"p\":0,\"q\":0}".into(), enc(&Pq { p: 0, q: 0 })),
        rec("region_of", "{\"p\":4,\"q\":-2}".into(), enc(&Pq { p: 4, q: -2 })),
        rec(
            "generate_province",
            format!("{{\"ring_seed\":\"{}\",\"p\":2,\"q\":0}}", to_hex(&s2)),
            enc(&GenerateArgs { ring_seed: s2, p: 2, q: 0 }),
        ),
        rec(
            "plan_path",
            format!("{{\"start\":{},\"dest\":{},\"unit\":0,\"blocked\":[],\"seeds\":{}}}", tile_json(start), tile_json(dest), seeds_json(&seeds3)),
            enc(&PlanArgs { start, dest, unit: UnitType::Spearman, blocked: vec![], seeds: seeds3.clone() }),
        ),
        rec(
            "path_cost",
            format!("{{\"start\":{},\"dirs\":{:?},\"unit\":2,\"seeds\":{}}}", tile_json(start), dirs, seeds_json(&seeds3)),
            enc(&PathCostArgs { start, dirs: dirs.clone(), unit: UnitType::Horseman, seeds: seeds3.clone() }),
        ),
        rec(
            "path_cost",
            format!("{{\"start\":{},\"dirs\":[0,7],\"unit\":0,\"seeds\":[]}}", tile_json(start)),
            enc(&PathCostArgs { start, dirs: vec![0, 7], unit: UnitType::Spearman, seeds: vec![] }),
        ),
        rec(
            "earliest_arrival_bell",
            format!("{{\"genesis_ts\":{g},\"depart_ts\":{},\"secs\":4000}}", g + 6_000),
            enc(&ArrivalArgs { genesis_ts: g, depart_ts: g + 6_000, secs: 4_000 }),
        ),
        rec(
            "check_arrival_bell",
            format!("{{\"genesis_ts\":{g},\"depart_ts\":{},\"secs\":4000,\"chosen\":11}}", g + 6_000),
            enc(&CheckArrivalArgs { genesis_ts: g, depart_ts: g + 6_000, secs: 4_000, chosen: 11 }),
        ),
        rec(
            "check_arrival_bell",
            format!("{{\"genesis_ts\":{g},\"depart_ts\":{},\"secs\":4000,\"chosen\":20}}", g + 6_000),
            enc(&CheckArrivalArgs { genesis_ts: g, depart_ts: g + 6_000, secs: 4_000, chosen: 20 }),
        ),
        rec("bell_at", format!("{{\"genesis_ts\":{g},\"t\":{}}}", g + 86_399), enc(&BellAtArgs { genesis_ts: g, t: g + 86_399 })),
        rec("bell_at", format!("{{\"genesis_ts\":{g},\"t\":{}}}", g - 1), enc(&BellAtArgs { genesis_ts: g, t: g - 1 })),
        rec("bell_start", format!("{{\"genesis_ts\":{g},\"bell\":1008}}"), enc(&BellStartArgs { genesis_ts: g, bell: 1_008 })),
        rec(
            "tlock_round",
            format!("{{\"drand\":{{\"genesis\":1692803367,\"period\":3}},\"genesis_ts\":{},\"bell\":{arrive}}}", c0.get("genesis_ts").i64()),
            enc(&TlockArgs { drand, genesis_ts: c0.get("genesis_ts").i64(), bell: arrive }),
        ),
        rec(
            "seed_round",
            format!("{{\"drand\":{{\"genesis\":1692803367,\"period\":3}},\"a\":{},\"window\":600,\"margin\":60}}", g + 1_234),
            enc(&SeedRoundArgs { drand, a: g + 1_234, window: 600, margin: 60 }),
        ),
        rec("plaintext_pack", fields_json, enc(&f)),
        rec("plaintext_unpack", format!("{{\"plain\":\"{}\"}}", to_hex(&plain)), plain.to_vec()),
        rec(
            "plaintext_validate",
            format!("{{\"plain\":\"{}\",\"host_id\":\"{host_id}\",\"arrive_bell\":{arrive}}}", to_hex(&plain)),
            enc(&ValidateArgs { plain, host_id, arrive_bell: arrive }),
        ),
        rec(
            "plaintext_validate",
            format!("{{\"plain\":\"{}\",\"host_id\":\"{}\",\"arrive_bell\":{arrive}}}", to_hex(&plain), host_id + 1),
            enc(&ValidateArgs { plain, host_id: host_id + 1, arrive_bell: arrive }),
        ),
        rec("commit", format!("{{\"plain\":\"{}\",\"salt\":\"{}\"}}", to_hex(&plain), to_hex(&salt)), enc(&CommitArgs { plain, salt })),
        rec("salt_of", format!("{{\"k\":\"{}\"}}", to_hex(&k)), enc(&KArgs { k })),
        rec("body_xor", format!("{{\"k\":\"{}\",\"plain\":\"{}\"}}", to_hex(&k), to_hex(&plain)), enc(&BodyArgs { k, plain })),
        rec("ct_hash", format!("{{\"seal\":\"{}\"}}", to_hex(&sealb)), sealb.to_vec()),
        rec(
            "seal_root",
            format!("{{\"commit\":\"{}\",\"ct_hash\":\"{}\"}}", c0.get("commit").str(), to_hex(&ct)),
            enc(&RootArgs { commit: hex(c0.get("commit").str()).try_into().expect("32"), ct_hash: ct }),
        ),
        rec("resolve_clash", "null".into(), enc(&clash_args())),
        {
            let c = &clash_model_cases()[0];
            rec(
                "resolve_from_inputs",
                format!(
                    "{{\"case\":\"{}\",\"province\":\"{}\",\"inputs\":\"{}\",\"bell\":{},\"seed\":\"{}\"}}",
                    c.name,
                    to_hex(&c.args.province),
                    to_hex(&c.args.inputs),
                    c.args.bell,
                    to_hex(&c.args.seed)
                ),
                enc(&c.args),
            )
        },
        rec(
            "reachable",
            format!("{{\"origin\":{{\"q\":0,\"r\":0}},\"dest\":{{\"q\":30,\"r\":0}},\"genesis_ts\":{g},\"depart_ts\":{},\"target_bell\":16,\"unit\":0}}", g + 6_000),
            enc(&ReachArgs { origin: Hex::new(0, 0), dest: Hex::new(30, 0), genesis_ts: g, depart_ts: g + 6_000, target_bell: 16, unit: UnitType::Spearman }),
        ),
        rec(
            "accrual_at",
            "{\"accrual\":{\"value\":\"5000\",\"rate\":\"3600000\",\"cap\":\"50000\",\"t0\":\"1000\",\"frac\":\"0\"},\"t\":\"4600\"}".into(),
            enc(&AccrualArgs { accrual: acc, t: 4_600 }),
        ),
    ]
}

fn vector_file() -> String {
    let mut out = String::new();
    out.push_str("{\n  \"generator\": \"frontier-wasm tests/exports.rs (PSF_WRITE_VECTORS=1 cargo test)\",\n");
    out.push_str(&format!(
        "  \"abi_version\": {},\n",
        frontier_wasm::ABI_VERSION
    ));
    out.push_str("  \"frame\": \"status u8 (0 ok, 1 bad input, 2 refused, 3 unavailable) | len u32 LE | payload\",\n");
    out.push_str(&format!(
        "  \"ruleset_hash\": \"{}\",\n",
        to_hex(&permutation_rules::frontier::ruleset_hash())
    ));
    out.push_str("  \"calls\": [\n");
    let recs = records();
    for (i, r) in recs.iter().enumerate() {
        let a = call(export_fn(r.export), &r.input);
        out.push_str(&format!(
            "    {{\"export\": \"{}\", \"args\": {}, \"input\": \"{}\", \"status\": {}, \"output\": \"{}\"}}{}\n",
            r.export,
            r.args,
            to_hex(&r.input),
            a.status,
            to_hex(&a.payload),
            if i + 1 < recs.len() { "," } else { "" }
        ));
    }
    out.push_str("  ]\n}\n");
    out
}

#[test]
fn wasm_vectors_are_fresh() {
    let path = root().join("frontier-wasm/vectors/wasm-vectors.json");
    let now = vector_file();
    json::parse(&now).expect("the vector file is JSON");
    if std::env::var("PSF_WRITE_VECTORS").as_deref() == Ok("1") {
        std::fs::write(&path, &now).expect("write vectors");
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        on_disk == now,
        "frontier-wasm/vectors/wasm-vectors.json is stale: run PSF_WRITE_VECTORS=1 cargo test"
    );
    // Every export has at least one recorded call.
    for name in frontier_wasm::EXPORTS {
        assert!(
            now.contains(&format!("\"export\": \"{name}\"")),
            "no vector for {name}"
        );
    }
}

// ------------------------------------------------------------------ resolve_from_inputs (W6-D)

/// Standard base64 (the vectors' account bytes), no dependency.
fn b64(s: &str) -> Vec<u8> {
    let val = |c: u8| -> u32 {
        match c {
            b'A'..=b'Z' => (c - b'A') as u32,
            b'a'..=b'z' => (c - b'a' + 26) as u32,
            b'0'..=b'9' => (c - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("base64 {c}"),
        }
    };
    let bytes: Vec<u8> = s.bytes().filter(|&c| c != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for ch in bytes.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in ch.iter().enumerate() {
            n |= val(c) << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if ch.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if ch.len() > 3 {
            out.push(n as u8);
        }
    }
    out
}

struct CmCase {
    name: String,
    args: FromInputsArgs,
    digest: [u8; 32],
    certain: bool,
}

/// The native clash model's cases (fclient `writes_frontier_vectors`,
/// `permutation-gateway/test/frontier-vectors.json` `clash_model`): account
/// bytes in, the native builder's outcome digest out.
fn clash_model_cases() -> Vec<CmCase> {
    let v = read_json("permutation-gateway/test/frontier-vectors.json");
    v.get("clash_model")
        .get("cases")
        .arr()
        .iter()
        .map(|c| CmCase {
            name: c.get("name").str().to_string(),
            args: FromInputsArgs {
                province: b64(c.get("province_b64").str()),
                inputs: b64(c.get("inputs_b64").str()),
                bell: c.get("bell").i64() as u32,
                seed: hex(c.get("seed").str()).try_into().expect("32"),
            },
            digest: hex(c.get("digest").str()).try_into().expect("32"),
            certain: matches!(c.get("certain"), Json::Bool(true)),
        })
        .collect()
}

#[test]
fn resolve_from_inputs_is_the_native_clash_model() {
    let cases = clash_model_cases();
    assert!(cases.len() >= 4);
    // The camp-check case is the one the page's transcription cannot model.
    assert!(cases.iter().any(|c| !c.certain));
    for c in &cases {
        let out: ClashOut = ok_of(go(frontier_wasm::resolve_from_inputs, &c.args));
        assert_eq!(
            out.digest, c.digest,
            "{}: the native builder's digest",
            c.name
        );
        assert_eq!(
            out.digest,
            out.outcome.digest(),
            "{}: the kernel's digest",
            c.name
        );
        assert_eq!(out.outcome.bell, c.args.bell, "{}", c.name);
        // The same answer as the program's model called directly.
        let built =
            frontier_abi::clash_model::build(&c.args.province, Some(&c.args.inputs), c.args.bell)
                .expect("build");
        let direct = clash::resolve_clash(&clash::frontier_ruleset(), &built.input(&c.args.seed))
            .expect("resolve");
        assert_eq!(direct, out.outcome, "{}", c.name);
        // Another seed is another outcome digest (the seed is used).
        let mut other = c.args.clone();
        other.seed[0] ^= 1;
        let o2: ClashOut = ok_of(go(frontier_wasm::resolve_from_inputs, &other));
        assert_eq!(o2.digest, o2.outcome.digest());
    }
}

#[test]
fn resolve_from_inputs_refuses_bad_accounts_with_codes() {
    let c = &clash_model_cases()[0];
    // A truncated Province: BadAccount (20).
    let mut a = c.args.clone();
    a.province.truncate(64);
    let r = go(frontier_wasm::resolve_from_inputs, &a);
    assert_eq!(r.status, REFUSED);
    assert_eq!(
        Refusal::try_from_slice(&r.payload).unwrap(),
        Refusal { code: 20, arg: 0 }
    );
    // A truncated ClashInputs: BadAccount too.
    let mut a = c.args.clone();
    a.inputs.truncate(10);
    let r = go(frontier_wasm::resolve_from_inputs, &a);
    assert_eq!(r.status, REFUSED);
    assert_eq!(Refusal::try_from_slice(&r.payload).unwrap().code, 20);
    // Trailing bytes after the borsh input: BAD_INPUT, not a panic.
    let mut raw = enc(&c.args);
    raw.push(0);
    assert_eq!(
        call(frontier_wasm::resolve_from_inputs, &raw).status,
        BAD_INPUT
    );
    assert_eq!(
        model_refusal(frontier_abi::clash_model::ModelError::Kernel(0x31)),
        (22, 0x31)
    );
    assert_eq!(
        model_refusal(frontier_abi::clash_model::ModelError::Overflow),
        (21, 0)
    );
}
