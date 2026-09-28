//! The exports' argument and answer types (borsh) and their bodies. Each
//! function takes the borsh input bytes and returns an [`Answer`]; the
//! kernels are `permutation_rules::frontier` unchanged, so the browser runs
//! the same code as the program and `frontier-node`.

use crate::{Answer, BAD_INPUT, REFUSED};
use borsh::{BorshDeserialize, BorshSerialize};
use permutation_rules::frontier::clash::{
    self, BeaconClock, ClashError, ClashInput, ClashOutcome, Fighter, Garrison, Occupancy,
    Relations,
};
use permutation_rules::frontier::geometry::{self, ProvinceCoord};
use permutation_rules::frontier::holding::Accrual;
use permutation_rules::frontier::seal::{self, Plain, PlainError, PLAIN_LEN, SEAL_LEN};
use permutation_rules::frontier::terrain::{self, ProvinceTerrain};
use permutation_rules::frontier::{beacon, travel};
use permutation_rules::hex::Hex;
use permutation_rules::units::UnitType;

/// A kernel refusal: `code` names the variant (tables below), `arg` its
/// argument where it has one (a step index, a bell, an id's low 32 bits).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Refusal {
    pub code: u8,
    pub arg: u32,
}

fn bad(reason: &str) -> Answer {
    Answer {
        status: BAD_INPUT,
        payload: reason.as_bytes().to_vec(),
    }
}

fn refused(code: u8, arg: u32) -> Answer {
    Answer {
        status: REFUSED,
        payload: borsh::to_vec(&Refusal { code, arg }).unwrap_or_default(),
    }
}

fn ok<T: BorshSerialize>(v: &T) -> Answer {
    match borsh::to_vec(v) {
        Ok(b) => Answer::ok(b),
        Err(_) => bad("encode"),
    }
}

/// Decode the whole input as `T` (trailing bytes are an error).
fn input<T: BorshDeserialize>(b: &[u8]) -> Option<T> {
    T::try_from_slice(b).ok()
}

macro_rules! take {
    ($b:expr, $t:ty) => {
        match input::<$t>($b) {
            Some(v) => v,
            None => return bad(concat!("input: ", stringify!($t))),
        }
    };
}

// ------------------------------------------------------------------ arguments

/// A province (P, Q).
#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct Pq {
    pub p: i32,
    pub q: i32,
}

/// A tile hex (axial q, r).
#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct Tile {
    pub q: i32,
    pub r: i32,
}

/// The ring seed of one opened ring.
#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct RingSeed {
    pub ring: u32,
    pub seed: [u8; 32],
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct GenerateArgs {
    pub ring_seed: [u8; 32],
    pub p: i32,
    pub q: i32,
}

/// A generated province: the kernel's terrain plus what the map draws.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct GeneratedProvince {
    pub terrain: ProvinceTerrain,
    /// Bit i set: tile i is passable.
    pub passable_mask: u64,
    pub centre: Hex,
    pub ring: u32,
    pub wedge: Option<u8>,
    pub region: u8,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct PlanArgs {
    pub start: Hex,
    pub dest: Hex,
    pub unit: UnitType,
    /// Hexes the path may not enter (shielded sites, the player's choice).
    pub blocked: Vec<Hex>,
    /// Ring seeds of the opened rings; a hex of any other ring is unknown
    /// land and never entered.
    pub seeds: Vec<RingSeed>,
}

/// A planned path: its directions (`hex::DIRECTIONS` order), the sealed
/// encoding (`seal::encode_path`) and its price.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PathPlan {
    pub dirs: Vec<u8>,
    pub path_len: u8,
    pub path: [u8; seal::PATH_BYTES],
    pub secs: u32,
    pub hexes: u32,
    pub provinces: Vec<ProvinceCoord>,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct PathCostArgs {
    pub start: Hex,
    pub dirs: Vec<u8>,
    pub unit: UnitType,
    pub seeds: Vec<RingSeed>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PathCostOut {
    pub secs: u32,
    pub hexes: u32,
    pub provinces: Vec<ProvinceCoord>,
    /// The destination hex (the last step).
    pub end: Hex,
}

#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct ArrivalArgs {
    pub genesis_ts: i64,
    pub depart_ts: i64,
    pub secs: u32,
}

#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct CheckArrivalArgs {
    pub genesis_ts: i64,
    pub depart_ts: i64,
    pub secs: u32,
    pub chosen: u32,
}

#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct BellAtArgs {
    pub genesis_ts: i64,
    pub t: i64,
}

#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct BellStartArgs {
    pub genesis_ts: i64,
    pub bell: u32,
}

/// The drand clock of the season (Season `drand_genesis`, `drand_period`).
#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct Drand {
    pub genesis: i64,
    pub period: u32,
}

impl Drand {
    fn clock(self) -> BeaconClock {
        BeaconClock {
            genesis: self.genesis,
            period: i64::from(self.period.max(1)),
        }
    }
}

#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct TlockArgs {
    pub drand: Drand,
    pub genesis_ts: i64,
    pub bell: u32,
}

/// `S = first_round_from(A + W + Δ)`.
#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct SeedRoundArgs {
    pub drand: Drand,
    /// THE anchor's `A`.
    pub a: i64,
    /// `W(b)`.
    pub window: u32,
    /// `Δ` (`seed_margin`).
    pub margin: u32,
}

/// `seal::Plain`, field for field (`reserved` included, so pack/unpack are
/// total).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PlainFields {
    pub version: u8,
    pub host_id: u64,
    pub arrive_bell: u32,
    pub dest_p: i16,
    pub dest_q: i16,
    pub dest_tile: u8,
    pub stance: u8,
    pub retreat_bps: u16,
    pub path_len: u8,
    pub path: [u8; seal::PATH_BYTES],
    pub reserved: [u8; seal::RESERVED_BYTES],
}

impl From<PlainFields> for Plain {
    fn from(f: PlainFields) -> Plain {
        Plain {
            version: f.version,
            host_id: f.host_id,
            arrive_bell: f.arrive_bell,
            dest_p: f.dest_p,
            dest_q: f.dest_q,
            dest_tile: f.dest_tile,
            stance: f.stance,
            retreat_bps: f.retreat_bps,
            path_len: f.path_len,
            path: f.path,
            reserved: f.reserved,
        }
    }
}

impl From<Plain> for PlainFields {
    fn from(p: Plain) -> PlainFields {
        PlainFields {
            version: p.version,
            host_id: p.host_id,
            arrive_bell: p.arrive_bell,
            dest_p: p.dest_p,
            dest_q: p.dest_q,
            dest_tile: p.dest_tile,
            stance: p.stance,
            retreat_bps: p.retreat_bps,
            path_len: p.path_len,
            path: p.path,
            reserved: p.reserved,
        }
    }
}

#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct ValidateArgs {
    pub plain: [u8; PLAIN_LEN],
    pub host_id: u64,
    pub arrive_bell: u32,
}

#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct CommitArgs {
    pub plain: [u8; PLAIN_LEN],
    pub salt: [u8; 32],
}

#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct KArgs {
    pub k: [u8; seal::K_LEN],
}

#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct BodyArgs {
    pub k: [u8; seal::K_LEN],
    pub plain: [u8; PLAIN_LEN],
}

#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct RootArgs {
    pub commit: [u8; 32],
    pub ct_hash: [u8; 32],
}

/// `ClashInput` with owned vectors (practice mode, "verify this clash").
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct ClashArgs {
    pub province: ProvinceCoord,
    pub bell: u32,
    pub seed: [u8; 32],
    pub terrain: ProvinceTerrain,
    pub residents: Vec<Fighter>,
    pub garrisons: Vec<Garrison>,
    pub arrivals: Vec<Fighter>,
    pub relations: Relations,
    pub occupancy: Occupancy,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct ClashOut {
    pub outcome: ClashOutcome,
    /// `ClashOutcome::digest` (the Province's recorded outcome digest).
    pub digest: [u8; 32],
}

/// Can a march from `origin` reach `dest` by `target_bell`? Optimistic:
/// the straight-line hex count at the unit's fastest open-ground pace, so a
/// "no" is certain and a "yes" is a warning, never a promise.
#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct ReachArgs {
    pub origin: Hex,
    pub dest: Hex,
    pub genesis_ts: i64,
    pub depart_ts: i64,
    pub target_bell: u32,
    pub unit: UnitType,
}

#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct AccrualArgs {
    pub accrual: Accrual,
    pub t: i64,
}

// ------------------------------------------------------------------ refusal codes
/// `TravelError` → `(code, arg)`: 1 EmptyPath, 2 TooLong, 3 NotAdjacent(i),
/// 4 Impassable(i), 5 OutOfBounds(i), 6 TooManyProvinces, 7 TooEarly
/// (earliest), 8 TooLate (latest); 9 = a step direction ≥ 6 (arg = step).
pub fn travel_refusal(e: travel::TravelError) -> (u8, u32) {
    use travel::TravelError::*;
    match e {
        EmptyPath => (1, 0),
        TooLong => (2, 0),
        NotAdjacent(i) => (3, u32::from(i)),
        Impassable(i) => (4, u32::from(i)),
        OutOfBounds(i) => (5, u32::from(i)),
        TooManyProvinces => (6, 0),
        TooEarly { earliest } => (7, earliest),
        TooLate { latest } => (8, latest),
    }
}

/// `PlainError` → code 1..=10 in declaration order (Version … Retreat).
pub fn plain_refusal(e: PlainError) -> u8 {
    use PlainError::*;
    match e {
        Version => 1,
        Reserved => 2,
        HostMismatch => 3,
        ArriveMismatch => 4,
        PathTooLong => 5,
        PathBits => 6,
        Direction => 7,
        Tile => 8,
        Stance => 9,
        Retreat => 10,
    }
}

/// `ClashError` → `(code, arg)`: 1 TooManyResidents, 2 TooManyArrivals,
/// 3 TooManyGarrisons, 4 BadTile, 5 BadFaction, 6 DuplicateId, 7 SharedTile,
/// 8 ArrivalWithoutStance, 9 TroopsAboveCap, 10 StaminaAboveCap,
/// 11 BadMultiplier, 12 BadRetreat (arg = the id's low 32 bits or the tile).
pub fn clash_refusal(e: ClashError) -> (u8, u32) {
    use ClashError::*;
    let lo = |id: u64| id as u32;
    match e {
        TooManyResidents => (1, 0),
        TooManyArrivals => (2, 0),
        TooManyGarrisons => (3, 0),
        BadTile(id) => (4, lo(id)),
        BadFaction(id) => (5, lo(id)),
        DuplicateId(id) => (6, lo(id)),
        SharedTile(t) => (7, u32::from(t)),
        ArrivalWithoutStance(id) => (8, lo(id)),
        TroopsAboveCap(id) => (9, lo(id)),
        StaminaAboveCap(id) => (10, lo(id)),
        BadMultiplier(id) => (11, lo(id)),
        BadRetreat(id) => (12, lo(id)),
    }
}

// ------------------------------------------------------------------ exports

pub fn abi_version(_: &[u8]) -> Answer {
    ok(&crate::ABI_VERSION)
}

/// `permutation_rules::frontier::ruleset_hash()` (= `frontier_abi::presets::RULESET_HASH`).
pub fn ruleset_hash(_: &[u8]) -> Answer {
    ok(&permutation_rules::frontier::ruleset_hash())
}

/// Tile hex → (province, tile index).
pub fn province_of(b: &[u8]) -> Answer {
    let t = take!(b, Tile);
    let (p, idx) = geometry::locate(Hex::new(t.q, t.r));
    ok(&(p, idx))
}

pub fn province_centre(b: &[u8]) -> Answer {
    let a = take!(b, Pq);
    ok(&ProvinceCoord::new(a.p, a.q).centre())
}

pub fn ring_of(b: &[u8]) -> Answer {
    let a = take!(b, Pq);
    ok(&ProvinceCoord::new(a.p, a.q).ring())
}

pub fn wedge_of(b: &[u8]) -> Answer {
    let a = take!(b, Pq);
    ok(&ProvinceCoord::new(a.p, a.q).wedge())
}

pub fn region_of(b: &[u8]) -> Answer {
    let a = take!(b, Pq);
    ok(&geometry::region_of(ProvinceCoord::new(a.p, a.q)))
}

/// The province's terrain from its ring seed (what OpenProvince writes).
pub fn generate_province(b: &[u8]) -> Answer {
    let a = take!(b, GenerateArgs);
    let p = match ProvinceCoord::checked(a.p, a.q, geometry::R_MAX_HARD) {
        Ok(p) => p,
        Err(_) => return refused(1, 0),
    };
    ok(&generated(&a.ring_seed, p))
}

pub(crate) fn generated(seed: &[u8; 32], p: ProvinceCoord) -> GeneratedProvince {
    let terrain = terrain::generate_province(seed, p);
    let mut passable_mask = 0u64;
    for i in 0..geometry::PROVINCE_TILES as u8 {
        if terrain.passable(i) {
            passable_mask |= 1 << i;
        }
    }
    GeneratedProvince {
        terrain,
        passable_mask,
        centre: p.centre(),
        ring: p.ring(),
        wedge: p.wedge(),
        region: geometry::region_of(p),
    }
}

/// The cheapest path (≤ 32 steps, ≤ 4 provinces) from `start` to `dest`,
/// or `None`. Client-side planning only: the program checks the path.
pub fn plan_path(b: &[u8]) -> Answer {
    let a = take!(b, PlanArgs);
    ok(&crate::path::plan(&a))
}

/// Price a path given as directions (the composer's waypoints).
pub fn path_cost(b: &[u8]) -> Answer {
    let a = take!(b, PathCostArgs);
    match crate::path::cost(&a) {
        Ok(c) => ok(&c),
        Err((code, arg)) => refused(code, arg),
    }
}

pub fn earliest_arrival_bell(b: &[u8]) -> Answer {
    let a = take!(b, ArrivalArgs);
    ok(&travel::earliest_arrival_bell(
        a.genesis_ts,
        a.depart_ts,
        a.secs,
    ))
}

pub fn check_arrival_bell(b: &[u8]) -> Answer {
    let a = take!(b, CheckArrivalArgs);
    match travel::check_arrival_bell(a.genesis_ts, a.depart_ts, a.secs, a.chosen) {
        Ok(()) => ok(&()),
        Err(e) => {
            let (code, arg) = travel_refusal(e);
            refused(code, arg)
        }
    }
}

/// `beacon::bell_at` (None before genesis).
pub fn bell_at(b: &[u8]) -> Answer {
    let a = take!(b, BellAtArgs);
    ok(&beacon::bell_at(a.genesis_ts, a.t))
}

pub fn bell_start(b: &[u8]) -> Answer {
    let a = take!(b, BellStartArgs);
    ok(&beacon::bell_start(a.genesis_ts, a.bell))
}

/// `T(b)`: the round a march arriving at `b` is sealed to.
pub fn tlock_round(b: &[u8]) -> Answer {
    let a = take!(b, TlockArgs);
    ok(&beacon::tlock_round(&a.drand.clock(), a.genesis_ts, a.bell))
}

/// `S(b, r) = first_round_from(A + W + Δ)`.
pub fn seed_round(b: &[u8]) -> Answer {
    let a = take!(b, SeedRoundArgs);
    let close = beacon::reveal_close(a.a, a.window);
    ok(&beacon::seed_round(&a.drand.clock(), close, a.margin))
}

pub fn plaintext_pack(b: &[u8]) -> Answer {
    let f = take!(b, PlainFields);
    ok(&seal::pack(&Plain::from(f)))
}

pub fn plaintext_unpack(b: &[u8]) -> Answer {
    let raw = take!(b, [u8; PLAIN_LEN]);
    ok(&PlainFields::from(seal::unpack(&raw)))
}

/// `seal::validate` (I-28): `()` or refusal `plain_refusal(e)`.
pub fn plaintext_validate(b: &[u8]) -> Answer {
    let a = take!(b, ValidateArgs);
    match seal::validate(&seal::unpack(&a.plain), a.host_id, a.arrive_bell) {
        Ok(()) => ok(&()),
        Err(e) => refused(plain_refusal(e), 0),
    }
}

pub fn commit(b: &[u8]) -> Answer {
    let a = take!(b, CommitArgs);
    ok(&seal::commit(&a.plain, &a.salt))
}

pub fn salt_of(b: &[u8]) -> Answer {
    let a = take!(b, KArgs);
    ok(&seal::salt_of(&a.k))
}

pub fn body_xor(b: &[u8]) -> Answer {
    let a = take!(b, BodyArgs);
    ok(&seal::body_xor(&a.k, &a.plain))
}

pub fn seal_root(b: &[u8]) -> Answer {
    let a = take!(b, RootArgs);
    ok(&seal::seal_root(&a.commit, &a.ct_hash))
}

pub fn ct_hash(b: &[u8]) -> Answer {
    let s = take!(b, [u8; SEAL_LEN]);
    ok(&seal::ct_hash(&s))
}

/// `clash::resolve_clash` under `frontier_ruleset()`, with the digest.
pub fn resolve_clash(b: &[u8]) -> Answer {
    let a = take!(b, ClashArgs);
    let inp = ClashInput {
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
    match clash::resolve_clash(&clash::frontier_ruleset(), &inp) {
        Ok(outcome) => {
            let digest = outcome.digest();
            ok(&ClashOut { outcome, digest })
        }
        Err(e) => {
            let (code, arg) = clash_refusal(e);
            refused(code, arg)
        }
    }
}

/// `resolve_from_inputs` input (contract §9.5, v1.6): the Province account
/// bytes before the resolve, the ClashInputs account bytes (gathered, with
/// the fate table the program wrote), the bell and THE anchor's seed; borsh
/// `(Vec<u8>, Vec<u8>, u32, [u8; 32])`.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct FromInputsArgs {
    pub province: Vec<u8>,
    pub inputs: Vec<u8>,
    pub bell: u32,
    pub seed: [u8; 32],
}

/// `ModelError` → `(code, arg)` of a `resolve_from_inputs` refusal: 20
/// BadAccount, 21 Overflow, 22 Kernel (arg = the program's `Kernel`
/// sub-code); a `ClashError` of the kernel keeps [`clash_refusal`]'s 1–12.
pub fn model_refusal(e: frontier_abi::clash_model::ModelError) -> (u8, u32) {
    use frontier_abi::clash_model::ModelError as M;
    match e {
        M::BadAccount => (20, 0),
        M::Overflow => (21, 0),
        M::Kernel(sub) => (22, sub as u32),
    }
}

/// ResolveFromInputs off chain (§9.5; wave-5 amendment "one clash model"):
/// the `ClashInput` is built from the account bytes by
/// `frontier_abi::clash_model::build` — the program's own builder (camp
/// check, garrisons, arrivals by ClashInputs position, occupancy) — then
/// resolved by the kernel under `frontier_ruleset()`; the digest is
/// `clash_model::outcome_digest`, the one the program records.
pub fn resolve_from_inputs(b: &[u8]) -> Answer {
    use frontier_abi::clash_model as m;
    let a = take!(b, FromInputsArgs);
    let built = match m::build(&a.province, Some(&a.inputs), a.bell) {
        Ok(x) => x,
        Err(e) => {
            let (code, arg) = model_refusal(e);
            return refused(code, arg);
        }
    };
    let outcome = match clash::resolve_clash(&clash::frontier_ruleset(), &built.input(&a.seed)) {
        Ok(o) => o,
        Err(e) => {
            let (code, arg) = clash_refusal(e);
            return refused(code, arg);
        }
    };
    match m::outcome_digest(&outcome) {
        Ok(digest) => ok(&ClashOut { outcome, digest }),
        Err(e) => {
            let (code, arg) = model_refusal(e);
            refused(code, arg)
        }
    }
}

pub fn reachable(b: &[u8]) -> Answer {
    let a = take!(b, ReachArgs);
    let hexes = a.origin.distance(a.dest);
    if hexes == 0 || hexes as usize > travel::MAX_PATH_STEPS {
        return ok(&false);
    }
    let depart_bell = travel::bell_at(a.genesis_ts, a.depart_ts);
    if a.target_bell > depart_bell.saturating_add(travel::MAX_ARRIVAL_LEAD_BELLS) {
        return ok(&false);
    }
    let secs = travel::open_ground_secs(hexes, false, a.unit).min(u64::from(u32::MAX)) as u32;
    let earliest = travel::earliest_arrival_bell(a.genesis_ts, a.depart_ts, secs);
    ok(&(earliest <= a.target_bell))
}

/// `Accrual::value_at(t)`: a store's value at `t` (the holding panel ticks).
pub fn accrual_at(b: &[u8]) -> Answer {
    let a = take!(b, AccrualArgs);
    ok(&a.accrual.value_at(a.t))
}
