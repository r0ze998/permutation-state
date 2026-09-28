//! `permutation-gateway/test/frontier-vectors.json` (M1 contract §8.1):
//! codec (tags, errors, log kinds, magics, sizes, every layout offset),
//! addresses, seal, fees, transaction shapes and beacon vectors, written by
//! `fclient`'s test `writes_frontier_vectors` and read by the JS SDK tests
//! (`frontier-*.test.mjs`, W2-D). Everything is deterministic except the
//! Rust-sealed tlock block (the stock crate draws its FO randomness): the
//! writer keeps the existing Rust seal when it still opens to the recorded
//! plaintext, so the file only changes when a vector changes.

use std::path::{Path, PathBuf};

use base64::Engine;
use serde_json::{json, Map, Value};
use solana_address::Address;
use solana_hash::Hash;
use solana_keypair::Keypair;
use solana_signer::Signer;

use crate::abi::{self, layout as l};
use crate::addr::{self, Addresses, SeedKind};
use crate::beacon;
use crate::fees;
use crate::ix;
use crate::seal;
use crate::tx;

pub fn path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../permutation-gateway/test/frontier-vectors.json")
}

macro_rules! offsets {
    ($m:ident ; $($f:ident),* $(,)?) => {{
        let mut o = Map::new();
        $( o.insert(stringify!($f).to_lowercase(), json!(l::$m::$f)); )*
        Value::Object(o)
    }};
}

fn layouts() -> Value {
    json!({
        "h": offsets!(h; MAGIC, SEASON_ID, LAYOUT_VERSION, EVENT_SEQ, EVENT_HEAD, LEN),
        "sh": offsets!(sh; MAGIC, SEASON_ID, LEN),
        "season": offsets!(season; STATUS, BUMP, REGIONS, GENESIS_RING, R_MAX, OFFICE_TERMS_PER_WALLET, POSTURES_ENABLED,
            AUTHORITY, RULESET_HASH, RULES_VERSION, PROGRAM_VERSION, BELL_SECS, GENESIS_TS, CREATED_TS, JOIN_CLOSE_BELL, END_BELL,
            DRAND_GENESIS, DRAND_PERIOD, NETWORK, QUICKNET_PK_HASH, REVEAL_WINDOW, SEED_MARGIN, WINDOW_NEXT, WINDOW_FROM_BELL,
            GENESIS_ROUND, GENESIS_SEED, ARCHIVE_AFTER, MIN_LEAD, MAX_LEAD, TRANSIT_SLOTS, MARCH_FEE, SEAL_BOND,
            MIN_REVEAL_PRIORITY_MILLI, REVEAL_CU_LIMIT, BUCKET_RATE_PER_H, BUCKET_BURST, DEFENCE_CAP_MILLI, LATENESS_SLOTS,
            THETA_EARLY_BPS, THETA_LATE_BPS, THETA_SWITCH_SECS, RESERVE_BPS, EXTRA_FREE_BPS, CLASH_CLOSE_GRACE,
            CAMP_REGROW_BELLS, PARAMS_HASH, T_CREATE_MIN, ANNOUNCED_TS, CREATION_BOND, PAYOUT_PARAMS_HASH, SHADE_AUDITOR,
            DORMANT_AFTER_SECS, RELEASE_AFTER_SECS, PFUND_INITIAL, DPOOL_INITIAL, REVEAL_LOADED_LIMIT, JOIN_GATE),
        "frontier": offsets!(frontier; RINGS_OPENED, LAST_RING_OPEN_BELL, LAST_RING_OPEN_TS, FOLD_BELL, FOLD_PART, OPEN_SITES,
            OCCUPIED_SITES, PROVINCES_OPENED, WEDGE_OPEN, WEDGE_OCCUPIED, ACC_OCCUPIED, ACC_WEDGE),
        "ring_seed": offsets!(ring_seed; D, STATUS, OPENED_BELL, T_OPEN, ROUND, SEED, PROVINCES_CREATED, PAYER),
        "province_fund": offsets!(province_fund; WEDGE, PROVINCES_OPENED, FUNDED_TOTAL, SPENT_TOTAL, OPEN_SITES, PROVINCES_FUNDED),
        "join_shard": offsets!(join_shard; FACTION, SHARD, MEMBERS, HOLDINGS, FINAL_HOLDINGS, HOLDINGS_BY_WEDGE, RELEASED),
        "beacon_log": offsets!(beacon_log; REGION, LATEST_ROUND, POSTED_TS, POSTED_SLOT, SIG48, BENEFICIARY),
        "defence_pool": offsets!(defence_pool; PAID_TOTAL, DIVERTED_TOTAL, PER_BELL_REGION_CAP, PER_KEEPER_DAY_CAP, CLAIMS),
        "citizen": offsets!(citizen; WALLET, SESSION, SESSION_EXPIRY, FACTION, FLAGS, HOLDINGS_N, EXPLORES_FLOOR_LEFT, JOIN_BELL,
            JOIN_SHARD, VIGIL_START_MIN, VIGIL_NEXT_MIN, VIGIL_FROM_TS, BUCKET_MILLI, BUCKET_T, HOLDING, HOLDING_STRIDE,
            OFFICE_TERMS_USED, TICKET_BELL, TICKET_SITES, TICKET_SITE_STRIDE, TICKET_NEXT, CITIZEN_TAG, LAST_ACTION_TS, WORKS,
            EXPLORES, ARRIVALS, RENT_PAYER, TICKET_ESCROW, TICKET_FUNDER),
        "holding": offsets!(holding; P, Q, SITE, GEN, TILE, STATE, OWNER_CITIZEN, TICKET_SCORE, FACTION, ORDER, TIER, FLAGS,
            TICKET_BELL, FOUNDED_TS, FOUNDED_DAY, HOST_SEQ, LAST_OWNER_ACTION, SHIELD_UNTIL, STORES, ACCRUAL_STRIDE, PRODUCTION,
            UPKEEP, QUEUE, QUEUE_STRIDE, WALLS, WALLS_COMMITTED_BEFORE, FOOD_SHORTFALL, RESERVE, DELEGATE, TRANSIT,
            TRANSIT_STRIDE, EXPLORE, ESCROW, RENT_PAYER, FINAL_TS, POOL_OWED),
        "transit": offsets!(transit; STATE, UNIT, FACTION, ORIGIN_TILE, ORIGIN_P, ORIGIN_Q, HOST_ID, DEPART_BELL, ARRIVE_BELL,
            DEPART_TS, DEP_MASS, MARCH_STAMINA, DEALT_BPS, TROOPS_AFTER, STAMINA_AFTER, READY_BELL_OFF, SEAL_ROOT, TIP, FLAGS),
        "explore": offsets!(explore; BELL, P, Q, TILES, HOST, STATE, LEN),
        "province": offsets!(province; P, Q, RING, WEDGE, REGION, RESOLVED_NEXT, OPENED_BELL, RELATIONS, LAST_OUTCOME_DIGEST,
            N_ENTRIES, N_SITES_USED, QUIET_OK, ROSTER_EPOCH, TERRAIN, RESOURCE, SITES, SITE_COUNT, PASSABLE_MASK, ROUGH_MASK,
            ROAD_MASK, EXPLORED_MASK, SITE_MIRROR, SITE_MIRROR_STRIDE, ENTRIES, ENTRY_STRIDE, LAST_RESOLVE, CAMP,
            TICKET_COHORTS, COHORT_STRIDE),
        "site": offsets!(site; STATE, FACTION, ORDER, TIER, GEN, GARRISON, PEND0_BELL, PEND0_DELTA, PEND1_BELL, PEND1_DELTA,
            WALLS_COMMITTED, WALL_ITEM0, WALL_ITEM1, SHIELD_UNTIL_BELL),
        "entry": offsets!(entry; ID, FACTION, UNIT, TILE, STATE, TROOPS, STAMINA_VALUE, DEALT_BPS, STAMINA_BELL, READY_BELL,
            FROM_BELL, PEND_BELL, PEND_OP, OP_A, OP_B, OP_TROOPS, OP_REF),
        "arrival_slot": offsets!(arrival_slot; P, Q, BELL, FACTION, I, UNIT, STANCE, TILE, FLAGS, RETREAT_BPS, HOST_ID,
            CITIZEN_TAG, DEP_MASS, DEALT_BPS, BENEFICIARY, RENT_TO, EV_SLOT, EV_PRICE, EV_LIMIT, EV_LOADED, CLAIMED),
        "arrival_day": offsets!(arrival_day; P, Q, DAY, BITS, RENT_TO),
        "clash_inputs": offsets!(clash_inputs; P, Q, BELL, ARRIVALS_MASK, POSTURE_MASK, FLAGS, N_PRESENT, SETTLED_MASK, ARRIVALS,
            ARRIVAL_STRIDE, POSTURES, RESOLVER, EV_SLOT, EV_PRICE, EV_LIMIT, RESOLVED_TS, RENT_TO),
        "arrival": offsets!(arrival; HOST_ID, CITIZEN_TAG, DEP_MASS, TROOPS, STAMINA, RETREAT, DEALT, FACTION, UNIT, TILE, STANCE,
            PRESENT, FATE, TROOPS_AFTER),
        "bell_anchor": offsets!(bell_anchor; BELL, REGION, NET, ROUND, A, SLOT, SIG48, RENT_TO, EV_PRICE, EV_LIMIT),
        "seed_cache": offsets!(seed_cache; BELL, REGION, NONCE, ROUND, SEED, ANCHOR_KEY, A, SLOT, RENT_TO),
        "anchor_archive": offsets!(anchor_archive; REGION, PART, TOMBSTONE, ARCHIVED, ENTRIES, ENTRY_STRIDE, RENT_TO),
        "defence_claim": offsets!(defence_claim; BENEFICIARY, DAY, CLAIMED, COUNT),
    })
}

fn codec() -> Value {
    use abi::{magic as mg, size as sz};
    let magics = json!({
        "season": String::from_utf8_lossy(mg::SEASON), "frontier": String::from_utf8_lossy(mg::FRONTIER),
        "ring_seed": String::from_utf8_lossy(mg::RING_SEED), "province_fund": String::from_utf8_lossy(mg::PROVINCE_FUND),
        "join_shard": String::from_utf8_lossy(mg::JOIN_SHARD), "beacon_log": String::from_utf8_lossy(mg::BEACON_LOG),
        "defence_pool": String::from_utf8_lossy(mg::DEFENCE_POOL), "citizen": String::from_utf8_lossy(mg::CITIZEN),
        "holding": String::from_utf8_lossy(mg::HOLDING), "province": String::from_utf8_lossy(mg::PROVINCE),
        "arrival_slot": String::from_utf8_lossy(mg::ARRIVAL_SLOT), "arrival_day": String::from_utf8_lossy(mg::ARRIVAL_DAY),
        "clash_inputs": String::from_utf8_lossy(mg::CLASH_INPUTS), "bell_anchor": String::from_utf8_lossy(mg::BELL_ANCHOR),
        "seed_cache": String::from_utf8_lossy(mg::SEED_CACHE), "anchor_archive": String::from_utf8_lossy(mg::ANCHOR_ARCHIVE),
        "defence_claim": String::from_utf8_lossy(mg::DEFENCE_CLAIM),
    });
    let sizes = json!({
        "season": sz::SEASON, "frontier": sz::FRONTIER, "ring_seed": sz::RING_SEED, "province_fund": sz::PROVINCE_FUND,
        "join_shard": sz::JOIN_SHARD, "beacon_log": sz::BEACON_LOG, "defence_pool": sz::DEFENCE_POOL, "citizen": sz::CITIZEN,
        "holding": sz::HOLDING, "province": sz::PROVINCE, "arrival_slot": sz::ARRIVAL_SLOT, "arrival_day": sz::ARRIVAL_DAY,
        "clash_inputs": sz::CLASH_INPUTS, "bell_anchor": sz::BELL_ANCHOR, "seed_cache": sz::SEED_CACHE,
        "anchor_archive": sz::ANCHOR_ARCHIVE, "defence_claim": sz::DEFENCE_CLAIM,
    });
    let rent: Map<String, Value> = sizes
        .as_object()
        .expect("obj")
        .iter()
        .map(|(k, v)| (k.clone(), json!(abi::rent(v.as_u64().expect("n") as usize))))
        .collect();
    json!({
        "tags": abi::INSTRUCTIONS.iter().map(|i| json!({"tag": i.tag, "name": i.name, "class": i.class.letter(),
            "cu_budget": i.cu_budget, "tx_max": i.tx_max, "top_level": i.top_level})).collect::<Vec<_>>(),
        "errors": abi::ERRORS.iter().map(|(c, n)| json!({"code": c, "name": n})).collect::<Vec<_>>(),
        "log_kinds": abi::kind::ALL.iter().map(|(k, n)| json!({"kind": k, "name": n})).collect::<Vec<_>>(),
        "entities": {"season": 1, "frontier": 2, "join_shard": 3, "citizen": 4, "holding": 5, "province": 6, "clash_inputs": 7},
        "seal_codes": {"valid": 0, "fo_failed": 1, "bad_point": 2, "wrong_round": 3, "commit_mismatch": 4, "plaintext_invalid": 5},
        "magics": magics,
        "sizes": sizes,
        "rent": rent,
        "layouts": layouts(),
    })
}

/// The program id every address vector uses.
pub fn vector_program() -> Address {
    use sha2::Digest;
    Address::new_from_array(sha2::Sha256::digest(b"PSF-VECTOR-PROGRAM").into())
}

fn addresses() -> Value {
    let a = Addresses::new(vector_program(), 1);
    let wallet = Address::new_from_array([0x5A; 32]);
    let row = |kind: SeedKind, raw: &[u8]| {
        json!({"kind": String::from_utf8_lossy(kind.tag()), "raw": hex::encode(raw), "seed": addr::seed_str(kind, raw),
               "address": a.of(kind, raw).to_string()})
    };
    let seeds = vec![
        row(SeedKind::Frontier, &[]),
        row(SeedKind::DefencePool, &[]),
        row(SeedKind::RingSeed, &addr::raw_ring_seed(0)),
        row(SeedKind::RingSeed, &addr::raw_ring_seed(128)),
        row(SeedKind::ProvinceFund, &[5]),
        row(SeedKind::JoinShard, &addr::raw_join_shard(5, 7)),
        row(SeedKind::BeaconLog, &[15]),
        row(SeedKind::Citizen, &addr::citizen_tag15(&wallet.to_bytes())),
        row(SeedKind::Holding, &addr::raw_holding(-3, 7, 11)),
        row(SeedKind::Province, &addr::raw_province(i32::MIN, i32::MAX)),
        row(SeedKind::Province, &addr::raw_province(-3, 7)),
        row(
            SeedKind::ArrivalSlot,
            &addr::raw_arrival_slot(-3, 7, u32::MAX, 5, 3),
        ),
        row(SeedKind::ArrivalDay, &addr::raw_arrival_day(-3, 7, 6)),
        row(
            SeedKind::ClashInputs,
            &addr::raw_clash_inputs(i32::MAX, i32::MIN, 1_007),
        ),
        row(SeedKind::BellAnchor, &addr::raw_anchor(u32::MAX, 15)),
        row(SeedKind::SeedCache, &addr::raw_seed_cache(1_007, 15, 255)),
        row(SeedKind::AnchorArchive, &addr::raw_archive(15, 6)),
        row(
            SeedKind::DefenceClaim,
            &addr::raw_defence_claim(&wallet.to_bytes(), 6),
        ),
        row(
            SeedKind::SealVerdict,
            &addr::raw_seal_verdict(u64::MAX, u32::MAX),
        ),
        row(SeedKind::Posture, &addr::raw_posture(-3, 7, 9, 2)),
    ];
    let hosts: Vec<Value> = [(0, 0, 0, 0, 0u32), (-3, 7, 11, 255, u32::MAX), (128, -64, 3, 1, 7)]
        .iter()
        .map(|&(p, q, s, g, n)| json!({"p": p, "q": q, "site": s, "gen": g, "seq": n, "id": addr::host_id(p, q, s, g, n).expect("host").to_string(),
            "holding": a.holding(p, q, s).to_string()}))
        .collect();
    json!({
        "program": a.program.to_string(),
        "season_id": a.season_id,
        "season": a.season.to_string(),
        "bump": a.bump,
        "programdata": a.programdata().to_string(),
        "seeds": seeds,
        "citizen": {"wallet": wallet.to_string(), "tag15": hex::encode(addr::citizen_tag15(&wallet.to_bytes())),
            "address": a.citizen(&wallet).to_string(), "citizen_tag": addr::citizen_tag_u64(&a.citizen(&wallet)).to_string(),
            "join_shard": addr::join_shard_of(&wallet.to_bytes()), "keeper_tag8": hex::encode(addr::keeper_tag8(&wallet.to_bytes()))},
        "host_ids": hosts,
    })
}

fn sample_plain() -> seal::Plain {
    seal::Plain {
        version: 1,
        host_id: addr::host_id(-3, 7, 11, 1, 42).expect("host"),
        arrive_bell: 147,
        dest_p: -2,
        dest_q: 7,
        dest_tile: 30,
        stance: 1,
        retreat_bps: 25_000,
        path_len: 7,
        path: seal::path_of(&[0, 0, 1, 5, 2, 3, 4]),
        reserved: [0; 3],
    }
}

fn plain_json(p: &seal::Plain) -> Value {
    json!({"version": p.version, "host_id": p.host_id.to_string(), "arrive_bell": p.arrive_bell, "dest_p": p.dest_p,
        "dest_q": p.dest_q, "dest_tile": p.dest_tile, "stance": p.stance, "retreat_bps": p.retreat_bps, "path_len": p.path_len,
        "path": hex::encode(p.path), "packed": hex::encode(seal::pack(p))})
}

fn seal_vectors(existing: Option<&Value>) -> Value {
    let q4: Value = serde_json::from_str(
        &std::fs::read_to_string(seal::fixture_dir().join("q4-vector.json")).expect("q4"),
    )
    .expect("json");
    let round = q4["round"].as_u64().expect("round");
    let sig48: [u8; 48] = hex::decode(q4["sig"].as_str().expect("sig"))
        .expect("hex")
        .try_into()
        .expect("48");
    let p = sample_plain();
    let pt = seal::pack(&p);
    let k: [u8; 16] = *b"PSF-vector-key16";
    let salt = seal::salt_of(&k);
    let body = seal::body_xor(&k, &pt);
    // Rust → JS: keep the stored seal while it still opens to this plaintext.
    let keep = existing
        .and_then(|e| e.get("seal"))
        .and_then(|s| s.get("rust_to_js"))
        .and_then(|r| {
            let seal_b: [u8; 165] = hex::decode(r.get("seal")?.as_str()?)
                .ok()?
                .try_into()
                .ok()?;
            (r.get("round")?.as_u64()? == round && seal::open(&seal_b, &sig48).ok()? == (k, pt))
                .then_some(seal_b)
        });
    let sealed = match keep {
        Some(s) => {
            let c = seal::commit(&pt, &salt);
            let h = seal::ct_hash(&s);
            seal::Sealed {
                seal: s,
                commit: c,
                salt,
                ct_hash: h,
                seal_root: seal::seal_root(&c, &h),
            }
        }
        None => seal::seal_with_key(&pt, &beacon::quicknet_info().public_key, round, &k)
            .expect("tlock encrypt"),
    };
    let js_k = seal::open_ibe(
        &hex::decode(q4["ct16"].as_str().expect("ct16")).expect("hex")[..seal::IBE_LEN],
        &sig48,
    )
    .expect("q4 opens");
    let mut invalid = vec![];
    for (name, f) in [
        (
            "version",
            Box::new(|x: &mut seal::Plain| x.version = 2) as Box<dyn Fn(&mut seal::Plain)>,
        ),
        (
            "reserved",
            Box::new(|x: &mut seal::Plain| x.reserved = [0, 1, 0]),
        ),
        ("path_len", Box::new(|x: &mut seal::Plain| x.path_len = 33)),
        (
            "path_bits",
            Box::new(|x: &mut seal::Plain| x.path[11] = 0x80),
        ),
        (
            "direction",
            Box::new(|x: &mut seal::Plain| x.path = seal::path_of(&[0, 0, 7, 0, 0, 0, 0])),
        ),
        ("tile", Box::new(|x: &mut seal::Plain| x.dest_tile = 61)),
        ("stance", Box::new(|x: &mut seal::Plain| x.stance = 4)),
        (
            "retreat",
            Box::new(|x: &mut seal::Plain| x.retreat_bps = 60_001),
        ),
    ] {
        let mut q = p;
        f(&mut q);
        invalid.push(json!({"why": name, "packed": hex::encode(seal::pack(&q)), "host_id": p.host_id.to_string(), "arrive_bell": p.arrive_bell}));
    }
    json!({
        "domains": {"march": "PS-FRONTIER-MARCH-v1", "posture": "PS-FRONTIER-POSTURE-v1", "salt": "PS-SALT", "keystream": "PS-KS"},
        "retreat_max_bps": seal::RETREAT_MAX_BPS,
        "plain": plain_json(&p),
        "k": hex::encode(k),
        "salt": hex::encode(salt),
        "commit": hex::encode(seal::commit(&pt, &salt)),
        "body": hex::encode(&body),
        "rust_to_js": {"round": round, "sig": hex::encode(sig48), "pk": beacon::QUICKNET_PK, "k": hex::encode(k),
            "plain": hex::encode(pt), "seal": hex::encode(sealed.seal), "commit": hex::encode(sealed.commit),
            "ct_hash": hex::encode(sealed.ct_hash), "seal_root": hex::encode(sealed.seal_root),
            "note": "sealed by the Rust tlock =0.0.10 crate; JS must open it with sig and get k and plain"},
        "js_to_rust": {"round": round, "sig": q4["sig"], "ct16": q4["ct16"], "commit16": q4["commit16"], "k": hex::encode(js_k),
            "note": "S-TLOCK q4: tlock-js compact-16 (69-B legacy plaintext) opened by the Rust crate; commitment matches"},
        "invalid_plaintexts": invalid,
    })
}

fn fee_vectors() -> Value {
    let cost_rows: Vec<Value> = [(26_000u32, 1u8, 2u8, fees::DEFAULT_LOADED_LIMIT), (16_000, 1, 2, 65_536), (340_000, 1, 3, 1_048_576), (345_000, 1, 2, 720_896)]
        .iter()
        .map(|&(l, s, w, ld)| json!({"limit": l, "sigs": s, "writes": w, "loaded": ld, "cost": fees::cost(l, s, w, ld)}))
        .collect();
    let tip_rows: Vec<Value> = [(433u32, 26_000u32, fees::DEFAULT_LOADED_LIMIT), (433, 16_000, fees::DEFAULT_LOADED_LIMIT), (433, 16_000, 65_536), (500, 21_000, 720_896)]
        .iter()
        .map(|&(p, l, ld)| json!({"p_milli": p, "limit": l, "loaded": ld, "tip_min": fees::min_tip_lamports(p, l, ld)}))
        .collect();
    let c = fees::cost(26_000, 1, 2, fees::DEFAULT_LOADED_LIMIT);
    let prio: Vec<Value> = [100u64, 433, 500, 1_000, 2_000]
        .iter()
        .map(|&p| {
            let fee = fees::fee_for(p, c);
            json!({"p_milli": p, "cost": c, "cu_limit": 26_000, "fee": fee, "cu_price_micro": fees::cu_price_micro(fee, 26_000),
                "priority_milli": fees::priority_milli(fee, c)})
        })
        .collect();
    let loaded: Vec<Value> = [(602_112u32, 20_000u32, 14u8), (10_000, 1_000, 3), (1_000_000, 50_000, 30)]
        .iter()
        .map(|&(pd, ab, n)| json!({"programdata_len": pd, "account_bytes": ab, "n_accounts": n, "loaded_limit": fees::loaded_limit(pd, ab, n)}))
        .collect();
    let s = fees::DefenceParams {
        defence_cap_milli: 2_000,
        tip_min: 14_441,
    };
    let refunds: Vec<Value> = [(0u64, false), (100_000, false), (1_000_000, true), (100_000_000, true)]
        .iter()
        .map(|&(price, day)| {
            let ev = fees::Evidence { ev_price: price, ev_limit: 26_000, ev_loaded: fees::DEFAULT_LOADED_LIMIT, created_day: day };
            json!({"ev_price": price, "ev_limit": 26_000, "ev_loaded": fees::DEFAULT_LOADED_LIMIT, "created_day": day,
                "defence_cap_milli": 2_000, "tip_min": 14_441, "refund": fees::defence_refund(&ev, &s)})
        })
        .collect();
    json!({
        "cost": cost_rows, "min_tip": tip_rows, "priority": prio, "loaded_limit": loaded, "defence_refund": refunds,
        "deploy_max_len": [{"so": 480_512, "max_len": fees::deploy_max_len(480_512)}, {"so": 540_608, "max_len": fees::deploy_max_len(540_608)}],
        "p_tip_milli": [{"tip": 14_441, "cost": c, "p_milli": fees::p_tip_milli(14_441, c)}],
    })
}

/// Deterministic keys for the shape vectors.
fn kp(n: u8) -> Keypair {
    Keypair::new_from_array([n; 32])
}

/// `(name, instruction, signers)` for every shape the web and relay build.
fn shapes() -> Value {
    let a = Addresses::new(vector_program(), 1);
    let (wallet, session, relay, keeper) = (kp(1), kp(2), kp(3), kp(4));
    let bh = Hash::new_from_array([0x11; 32]);
    let budgets = crate::budgets::Budgets::canonical();
    let h = ix::HoldingRef {
        p: -3,
        q: 7,
        site: 11,
    };
    let pl = ix::Player {
        actor: session.pubkey(),
        payer: relay.pubkey(),
        wallet: wallet.pubkey(),
    };
    let p = sample_plain();
    let sealed_seal = [0xC0u8; 165];
    let dep = ix::DepartArgs {
        host_id: p.host_id,
        commit: [0xAA; 32],
        seal: sealed_seal,
        arrive_bell: 147,
        tip: 14_441,
        transit_slot: 1,
    };
    let rv = ix::RevealArgs {
        holding: h,
        transit_slot: 1,
        target_i: 2,
        plain: seal::pack(&p),
        salt: [0xBB; 32],
        ct_hash: [0xCC; 32],
        beneficiary: keeper.pubkey(),
        dest: (-2, 7),
        arrive: 147,
        faction: 3,
        day_writable: true,
        path_provinces: vec![(-3, 7), (-3, 8), (-2, 8)],
    };
    let st = ix::SettleTransitArgs {
        holding: h,
        transit_slot: 1,
        commit: [0xAA; 32],
        seal: sealed_seal,
        beneficiary: relay.pubkey(),
        dest: (-2, 7),
        arrive: 147,
        faction: 3,
        slot_i: 2,
        home: (-3, 7),
        anchor_present: false,
        slot_beneficiary: keeper.pubkey(),
        resolver: keeper.pubkey(),
        holding_rent_payer: relay.pubkey(),
        camp_citizen: None,
    };
    let fixture = beacon::FixtureDrand::load(&beacon::fixture_dir(), beacon::quicknet_info())
        .expect("fixtures");
    let b = fixture.rounds.values().next().expect("a round");
    let barg = beacon::beacon_arg(b);
    let cases: Vec<(&str, solana_instruction::Instruction, Vec<&Keypair>)> = vec![
        (
            "join",
            ix::join(
                &a,
                wallet.pubkey(),
                relay.pubkey(),
                3,
                &session.pubkey(),
                1_900_000_000,
                None,
            ),
            vec![&relay, &wallet],
        ),
        (
            "file_ticket",
            ix::file_ticket(
                &a,
                &pl,
                &[
                    ix::Site {
                        p: -3,
                        q: 7,
                        site: 11,
                    },
                    ix::Site {
                        p: -3,
                        q: 8,
                        site: 2,
                    },
                    ix::Site {
                        p: -2,
                        q: 7,
                        site: 0,
                    },
                ],
            ),
            vec![&relay, &session],
        ),
        ("harvest", ix::harvest(&a, &pl, h), vec![&relay, &session]),
        (
            "muster",
            ix::muster(&a, &pl, h, 2, 1_000, 30),
            vec![&relay, &session],
        ),
        (
            "depart",
            ix::depart(&a, &pl, h, (-3, 7), &dep),
            vec![&relay, &session],
        ),
        (
            "reveal",
            ix::reveal(&a, keeper.pubkey(), &rv),
            vec![&keeper],
        ),
        (
            "settle_transit",
            ix::settle_transit(&a, relay.pubkey(), &st),
            vec![&relay],
        ),
        (
            "post_anchor",
            ix::post_anchor(&a, keeper.pubkey(), 5, 147, &barg, &keeper.pubkey()),
            vec![&keeper],
        ),
    ];
    let b64 = base64::engine::general_purpose::STANDARD;
    let rows: Vec<Value> = cases
        .into_iter()
        .map(|(name, ixn, signers)| {
            let tag = ixn.data[0];
            let player = abi::ix_info(tag).map(|i| i.class == abi::Class::P).unwrap_or(false) || tag == abi::tag::SETTLE_TRANSIT;
            let budget = tx::TxBudget::from_budget(budgets.get(tag), 0);
            let budget = if player { budget } else { tx::TxBudget { cu_price: fees::cu_price_for(2_000, 30_000, budget.cu_limit), ..budget } };
            let t = tx::build(&[ixn], &budget, &signers, &bh).expect("sign");
            let s = tx::shape(&t.message);
            let info = abi::ix_info(tag).expect("tag");
            assert!(s.bytes <= abi::PACKET && s.locks <= abi::LOCK_LIMIT, "{name}: {} B, {} locks", s.bytes, s.locks);
            json!({"name": name, "tag": tag, "bytes": s.bytes, "locks": s.locks, "writes": s.writes, "sigs": s.sigs,
                "tx_max": info.tx_max, "within_tx_max": s.bytes <= info.tx_max as usize, "cu_limit": budget.cu_limit, "cu_price": budget.cu_price, "loaded_limit": budget.loaded_limit,
                "fee_payer": signers[0].pubkey().to_string(), "wire": b64.encode(tx::wire(&t))})
        })
        .collect();
    json!({"blockhash": bh.to_string(), "keys": {"wallet": wallet.pubkey().to_string(), "session": session.pubkey().to_string(),
        "relay": relay.pubkey().to_string(), "keeper": keeper.pubkey().to_string()}, "cases": rows})
}

fn beacon_vectors() -> Value {
    let q = beacon::quicknet_info();
    let t = beacon::TestKey::new();
    let fixture = beacon::FixtureDrand::load(&beacon::fixture_dir(), q.clone()).expect("fixtures");
    let (r0, b0) = fixture.rounds.iter().next().expect("round");
    let sig96 = beacon::decompress_sig(&b0.sig48).expect("sig");
    json!({
        "quicknet": beacon::info_json(&q),
        "quicknet_pk_hash": hex::encode(beacon::pk_hash(&q.public_key)),
        "fixture_round": {"round": r0, "sig": hex::encode(b0.sig48), "sig96": hex::encode(sig96), "seed": hex::encode(beacon::seed_of(*r0, &sig96)),
            "hints": hex::encode(beacon::hints_bytes(*r0)), "msg": hex::encode(beacon::msg(*r0))},
        "test_key": {"info": beacon::info_json(&t.info()), "pk_hash": hex::encode(beacon::pk_hash(&t.pk96)),
            "keygen": {"msg": "PSF-TEST-BEACON-v1", "dst": "PSF-TEST-BEACON-KEYGEN-v1", "rule": "sk = OS2IP(expand_message_xmd_sha256(msg, dst, 48)) mod r"},
            "rounds": ([1u64, 1_000, 32_556_350].iter().map(|&r| json!({"round": r, "sig": hex::encode(t.sign(r))})).collect::<Vec<_>>())},
        "clock": ([0u32, 1, 143, 144, 1_007].iter().map(|&b| {
            let g = 1_800_000_000i64;
            let d = crate::clock::Drand::QUICKNET;
            json!({"genesis_ts": g, "bell": b, "bell_end": crate::clock::bell_end(g, b), "tlock_round": d.tlock_round(g, b),
                "seed_round_at_a_plus_2": d.seed_round(crate::clock::reveal_close(crate::clock::bell_end(g, b) + 2, 600), 60)})
        }).collect::<Vec<_>>()),
    })
}

/// The whole file, reusing `existing`'s Rust seal when still valid.
pub fn build(existing: Option<&Value>) -> Value {
    json!({
        "v": 1,
        "generator": "frontier-node/crates/fclient/src/vectors.rs (test writes_frontier_vectors)",
        "contract": "docs/frontier/m1/M1-CONTRACT.md v1.1",
        "codec": codec(),
        "addresses": addresses(),
        "seal": seal_vectors(existing),
        "fees": fee_vectors(),
        "shapes": shapes(),
        "beacon": beacon_vectors(),
        "clash_model": clash_model_vectors(),
    })
}

// ------------------------------------------------------------------ the §5.11 builder
// Integ-W4 review (W4-E): the web page's `buildClashArgs` is pinned against
// the native builder (`clash_model::build`, the program's model as the
// herald and the verifier read it) on account bytes, not on a JS copy of
// the rules: the camp's whole troops × 1,000 capped, the camp only below
// MAX_GARRISONS, a zero-delta pending garrison slot ignored, `dealt` 0
// reading 1.0, residents and arrivals in id order, `certain` exactly when
// the day's camp check does not run.

struct Mirror {
    site: u8,
    faction: u8,
    gen: u8,
    garrison: u32,
    pend0: (u32, i64),
    walls: u32,
}

struct Res {
    id: u64,
    faction: u8,
    unit: u8,
    tile: u8,
    troops: u32,
    stamina: (u16, u32),
    dealt: u16,
    from_bell: u32,
}

struct Arr {
    id: u64,
    faction: u8,
    unit: u8,
    tile: u8,
    troops: u32,
    stamina: u16,
    retreat: u16,
    dealt: u16,
    stance: u8,
    present: u8,
}

struct CaseSpec {
    name: &'static str,
    bell: u32,
    mirrors: Vec<Mirror>,
    /// (tile, state, whole troops, next_check_day, gen)
    camp: (u8, u8, u32, u32, u32),
    residents: Vec<Res>,
    muster_pending: u8,
    arrivals: Vec<Arr>,
}

const VP: i32 = 2;
const VQ: i32 = 0;

fn vhid(p: i32, q: i32, site: u8, gen: u8, seq: u32) -> u64 {
    crate::addr::host_id(p, q, site, gen, seq).expect("host id")
}

fn province_of(c: &CaseSpec) -> Vec<u8> {
    use frontier_abi::entry::{write_entry, Entry, EntryOp};
    use l::province as o;
    let mut d = vec![0u8; crate::abi::size::PROVINCE];
    d[l::h::MAGIC..l::h::MAGIC + 8].copy_from_slice(crate::abi::magic::PROVINCE);
    let put = |d: &mut Vec<u8>, at: usize, b: &[u8]| d[at..at + b.len()].copy_from_slice(b);
    put(&mut d, o::P, &(VP as i16).to_le_bytes());
    put(&mut d, o::Q, &(VQ as i16).to_le_bytes());
    put(&mut d, o::RING, &2u16.to_le_bytes());
    put(&mut d, o::RESOLVED_NEXT, &c.bell.to_le_bytes());
    for i in 0..61 {
        // Grassland, a forest and a hill; wheat on tile 9.
        d[o::TERRAIN + i] = match i {
            7 => 2,
            30 => 3,
            _ => 0,
        };
        d[o::RESOURCE + i] = u8::from(i == 9);
    }
    let sites: [u8; 12] = [2, 5, 9, 14, 20, 24, 30, 36, 41, 47, 52, 58];
    put(&mut d, o::SITES, &sites);
    d[o::SITE_COUNT] = 12;
    for m in &c.mirrors {
        let b = o::SITE_MIRROR + m.site as usize * o::SITE_MIRROR_STRIDE;
        use frontier_abi::layout::province::site as st;
        d[b + st::STATE] = frontier_abi::layout::province::site::STATE_HOLDING;
        d[b + st::FACTION] = m.faction;
        d[b + st::GEN] = m.gen;
        put(&mut d, b + st::GARRISON, &m.garrison.to_le_bytes());
        put(&mut d, b + st::PEND0_BELL, &m.pend0.0.to_le_bytes());
        put(&mut d, b + st::PEND0_DELTA, &m.pend0.1.to_le_bytes());
        put(
            &mut d,
            b + st::PEND1_BELL,
            &frontier_abi::layout::province::site::NO_BELL.to_le_bytes(),
        );
        put(&mut d, b + st::WALLS_COMMITTED, &m.walls.to_le_bytes());
        put(
            &mut d,
            b + st::WALL_ITEM0_BELL,
            &frontier_abi::layout::province::site::NO_BELL.to_le_bytes(),
        );
        put(
            &mut d,
            b + st::WALL_ITEM1_BELL,
            &frontier_abi::layout::province::site::NO_BELL.to_le_bytes(),
        );
    }
    let mut n = 0;
    for r in &c.residents {
        let e = Entry {
            id: r.id,
            faction: r.faction,
            unit: r.unit,
            tile: r.tile,
            state: 1,
            troops: r.troops,
            stamina_value: r.stamina.0,
            dealt_bps: r.dealt,
            stamina_bell: r.stamina.1,
            ready_bell: 0,
            from_bell: r.from_bell,
            pend_bell: 0,
            op: EntryOp::None,
        };
        write_entry(&mut d, n, &e).expect("entry");
        n += 1;
    }
    for k in 0..c.muster_pending {
        let e = Entry {
            id: vhid(VP, VQ, 3, 2, 90 + k as u32),
            faction: 0,
            unit: 1,
            tile: 14,
            state: 2,
            troops: 100_000,
            stamina_value: 120,
            dealt_bps: 0,
            stamina_bell: 0,
            ready_bell: 0,
            from_bell: c.bell + 1,
            pend_bell: 0,
            op: EntryOp::None,
        };
        write_entry(&mut d, n, &e).expect("entry");
        n += 1;
    }
    d[o::N_ENTRIES] = n as u8;
    let (tile, state, troops, next, gen) = c.camp;
    d[o::CAMP] = tile;
    d[o::CAMP + 1] = state;
    put(&mut d, o::CAMP + 4, &troops.to_le_bytes());
    put(&mut d, o::CAMP + 8, &next.to_le_bytes());
    put(&mut d, o::CAMP + 12, &gen.to_le_bytes());
    d
}

fn inputs_of(c: &CaseSpec) -> Vec<u8> {
    use l::arrival as a;
    use l::clash_inputs as ci;
    let mut d = vec![0u8; crate::abi::size::CLASH_INPUTS];
    d[l::h::MAGIC..l::h::MAGIC + 8].copy_from_slice(crate::abi::magic::CLASH_INPUTS);
    let put = |d: &mut Vec<u8>, at: usize, b: &[u8]| d[at..at + b.len()].copy_from_slice(b);
    put(&mut d, ci::P, &(VP as i16).to_le_bytes());
    put(&mut d, ci::Q, &(VQ as i16).to_le_bytes());
    put(&mut d, ci::BELL, &c.bell.to_le_bytes());
    d[ci::FLAGS] = 1;
    let mut mask = 0u32;
    for (k, r) in c.arrivals.iter().enumerate() {
        let b = ci::ARRIVALS + k * ci::ARRIVAL_STRIDE;
        put(&mut d, b + a::HOST_ID, &r.id.to_le_bytes());
        put(&mut d, b + a::DEP_MASS, &(r.troops / 1000).to_le_bytes());
        put(&mut d, b + a::TROOPS, &r.troops.to_le_bytes());
        put(&mut d, b + a::STAMINA, &r.stamina.to_le_bytes());
        put(&mut d, b + a::RETREAT, &r.retreat.to_le_bytes());
        put(&mut d, b + a::DEALT, &r.dealt.to_le_bytes());
        d[b + a::FACTION] = r.faction;
        d[b + a::UNIT] = r.unit;
        d[b + a::TILE] = r.tile;
        d[b + a::STANCE] = r.stance;
        d[b + a::PRESENT] = r.present;
        if r.present == 1 {
            mask |= 1 << k;
        }
    }
    put(&mut d, ci::ARRIVALS_MASK, &mask.to_le_bytes());
    d[ci::N_PRESENT] = mask.count_ones() as u8;
    d
}

fn clash_cases() -> Vec<CaseSpec> {
    let m = |site, faction, gen, garrison| Mirror {
        site,
        faction,
        gen,
        garrison,
        pend0: (frontier_abi::layout::province::site::NO_BELL, 0),
        walls: 0,
    };
    let base_res = |bell: u32| {
        vec![
            // Stored out of id order; one with dealt 0 (reads 1.0).
            Res {
                id: vhid(VP, VQ, 5, 0, 7),
                faction: 1,
                unit: 2,
                tile: 20,
                troops: 400_000,
                stamina: (90, bell - 5),
                dealt: 0,
                from_bell: bell - 3,
            },
            Res {
                id: vhid(VP, VQ, 3, 2, 1),
                faction: 0,
                unit: 0,
                tile: 14,
                troops: 800_000,
                stamina: (118, bell - 5),
                dealt: 10_500,
                from_bell: bell - 3,
            },
            Res {
                id: vhid(VP, VQ, 3, 2, 4),
                faction: 0,
                unit: 6,
                tile: 15,
                troops: 150_000,
                stamina: (40, bell),
                dealt: 10_000,
                from_bell: bell + 1,
            },
        ]
    };
    let base_arr = || {
        vec![
            Arr {
                id: vhid(4, -2, 8, 0, 3),
                faction: 2,
                unit: 5,
                tile: 14,
                troops: 400_000,
                stamina: 70,
                retreat: 15_000,
                dealt: 11_000,
                stance: 3,
                present: 1,
            },
            Arr {
                id: vhid(4, -2, 7, 0, 1),
                faction: 1,
                unit: 0,
                tile: 14,
                troops: 900_000,
                stamina: 60,
                retreat: 0,
                dealt: 0,
                stance: 1,
                present: 1,
            },
            Arr {
                id: vhid(4, -2, 9, 0, 1),
                faction: 2,
                unit: 0,
                tile: 20,
                troops: 300_000,
                stamina: 70,
                retreat: 0,
                dealt: 10_000,
                stance: 2,
                present: 0,
            },
            Arr {
                id: vhid(4, -2, 6, 1, 2),
                faction: 3,
                unit: 1,
                tile: 33,
                troops: 600_000,
                stamina: 100,
                retreat: 0,
                dealt: 10_000,
                stance: 2,
                present: 1,
            },
        ]
    };
    let mut big = m(3, 0, 2, 40_000_000);
    big.pend0 = (10, 0); // an earlier bell, delta 0: no change, ignored
    big.walls = 1;
    vec![
        CaseSpec {
            name: "camp_capped_ids_unsorted",
            bell: 40,
            mirrors: vec![big, m(5, 1, 0, 100_000)],
            camp: (33, 1, 50_000, 1, 3),
            residents: base_res(40),
            muster_pending: 1,
            arrivals: base_arr(),
        },
        CaseSpec {
            name: "camp_small_certain",
            bell: 150,
            mirrors: vec![m(5, 1, 0, 100_000)],
            camp: (33, 1, 250, 2, 4),
            residents: base_res(150),
            muster_pending: 0,
            arrivals: base_arr(),
        },
        CaseSpec {
            name: "twelve_garrisons_leave_no_camp_slot",
            bell: 40,
            mirrors: (0..12)
                .map(|s| m(s, s % 6, 0, 50_000 + s as u32 * 1000))
                .collect(),
            camp: (33, 1, 250, 1, 3),
            residents: base_res(40),
            muster_pending: 0,
            arrivals: base_arr(),
        },
        CaseSpec {
            name: "camp_check_due_uncertain",
            bell: 150,
            mirrors: vec![m(5, 1, 0, 100_000)],
            camp: (33, 0, 0, 1, 3),
            residents: base_res(150),
            muster_pending: 0,
            arrivals: base_arr(),
        },
    ]
}

fn clash_model_vectors() -> Value {
    let b64 = |b: &[u8]| base64::engine::general_purpose::STANDARD.encode(b);
    let cases: Vec<Value> = clash_cases()
        .iter()
        .enumerate()
        .map(|(k, c)| {
            let pd = province_of(c);
            let cd = inputs_of(c);
            let seed = permutation_rules::hash::sha256(&[b"clash-model-vector", &[k as u8]]);
            let built = crate::clash_model::build(&pd, &cd, c.bell, &seed).expect("build");
            let out = built.resolve().expect("resolve");
            let posture = |p: &permutation_rules::frontier::stance::Posture| format!("{p:?}");
            json!({
                "name": c.name,
                "bell": c.bell,
                "seed": hex::encode(seed),
                "province_b64": b64(&pd),
                "inputs_b64": b64(&cd),
                "certain": !built.camp.checked,
                "camp_checked": built.camp.checked,
                "residents": built.residents.iter().map(|f| json!([f.id.to_string(), f.troops, f.stamina, f.dealt_bps, posture(&f.posture)])).collect::<Vec<_>>(),
                "garrisons": built.garrisons.iter().map(|g| json!([g.id.to_string(), g.faction, g.tile, g.troops, g.walls])).collect::<Vec<_>>(),
                "arrivals": built.arrivals.iter().map(|f| json!([f.id.to_string(), f.troops, f.stamina, f.dealt_bps, f.retreat_bps, posture(&f.posture)])).collect::<Vec<_>>(),
                "occupancy": {"pending": built.occupancy.pending.to_vec(), "storage_free": built.occupancy.storage_free},
                "digest": hex::encode(out.digest()),
                "fighters": out.fighters.iter().map(|f| json!([f.id.to_string(), f.arrival, f.troops])).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "rule": "fclient clash_model::build over (Province bytes, ClashInputs bytes, bell, seed); `certain` = the day's camp check does not run at this bell",
        "cases": cases,
    })
}

/// Writes the file if it changed; returns whether it did.
pub fn write() -> std::io::Result<bool> {
    let p = path();
    let old = std::fs::read_to_string(&p).ok();
    let existing = old
        .as_deref()
        .and_then(|s| serde_json::from_str::<Value>(s).ok());
    let mut text = serde_json::to_string_pretty(&build(existing.as_ref())).expect("json");
    text.push('\n');
    if old.as_deref() == Some(text.as_str()) {
        return Ok(false);
    }
    std::fs::write(&p, text)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_frontier_vectors() {
        let changed = write().expect("write vectors");
        // A second build over the written file is identical (determinism).
        let text = std::fs::read_to_string(path()).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        let mut again = serde_json::to_string_pretty(&build(Some(&v))).unwrap();
        again.push('\n');
        assert_eq!(again, text, "vectors are deterministic");
        if std::env::var("FRONTIER_VECTORS_CHECK").is_ok() {
            assert!(
                !changed,
                "frontier-vectors.json was stale; commit the regenerated file"
            );
        }
        // The Rust seal in the file opens with the recorded signature.
        let r = &v["seal"]["rust_to_js"];
        let s: [u8; 165] = hex::decode(r["seal"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap();
        let sig: [u8; 48] = hex::decode(r["sig"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap();
        let (k, pt) = seal::open(&s, &sig).unwrap();
        assert_eq!(hex::encode(k), r["k"].as_str().unwrap());
        assert_eq!(hex::encode(pt), r["plain"].as_str().unwrap());
        // Depart stays ≤ 800 B (web-frontier-march.test.mjs asserts it too).
        let dep = v["shapes"]["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "depart")
            .unwrap();
        assert!(dep["bytes"].as_u64().unwrap() <= 800);
    }
}
