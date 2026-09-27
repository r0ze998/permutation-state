//! Rules v10: the Sixfold Frontier (open-world design, revision 2).
//!
//! A separate module tree next to the v9 (Skirmish) rules, which it does
//! not change (design §11.1). Kernels here are pure functions of their
//! inputs, integer only, so the Frontier program, the verifier and the
//! host simulator call the same code.

/// Rules version of the Frontier. v9 (`params::RULES_VERSION`) stays the
/// Skirmish rules; golden tests pin both.
pub const RULES_VERSION_FRONTIER: u16 = 10;

pub mod addr;
pub mod beacon;
pub mod camp;
pub mod catalog;
pub mod clash;
pub mod doctrine;
pub mod explore;
pub mod fees;
pub mod geometry;
pub mod holding;
pub mod host;
pub mod index;
pub mod laurel;
pub mod mandate;
pub mod office;
pub mod payout;
pub mod pools;
pub mod seal;
pub mod siege;
pub mod stance;
pub mod terrain;
pub mod travel;

/// Domain of the ruleset hash input (version 1 of its layout).
pub const RULESET_DOMAIN: &[u8] = b"PSF-RULESET-v1";

/// The kernel version constants the ruleset hash binds, in pinned order
/// (append only): one per module of this tree, so a kernel whose honest
/// outcomes change bumps its constant and the hash moves (integ-W1 review:
/// the first nine left clash, siege, doctrine, … out).
pub const KERNEL_VERSIONS: [(&str, u16); 23] = [
    ("frontier", RULES_VERSION_FRONTIER),
    ("beacon", beacon::BEACON_VERSION),
    ("addr", addr::ADDR_VERSION),
    ("seal", seal::SEAL_VERSION),
    ("fees", fees::FEES_VERSION),
    ("office", office::OFFICE_VERSION),
    ("camp", camp::CAMP_VERSION),
    ("explore", explore::EXPLORE_VERSION),
    ("catalog", catalog::CATALOG_VERSION),
    ("clash", clash::CLASH_VERSION),
    ("siege", siege::SIEGE_VERSION),
    ("doctrine", doctrine::DOCTRINE_VERSION),
    ("terrain", terrain::TERRAIN_VERSION),
    ("travel", travel::TRAVEL_VERSION),
    ("holding", holding::HOLDING_VERSION),
    ("host", host::HOST_VERSION),
    ("geometry", geometry::GEOMETRY_VERSION),
    ("stance", stance::STANCE_VERSION),
    ("index", index::INDEX_VERSION),
    ("laurel", laurel::LAUREL_VERSION),
    ("mandate", mandate::MANDATE_VERSION),
    ("payout", payout::PAYOUT_VERSION),
    ("pools", pools::POOLS_VERSION),
];

/// The rule constants of the clash, host, holding, siege and travel
/// kernels bound into `RULESET_HASH` (pinned order, append only), each as
/// an `i64`.
pub const KERNEL_CONSTANTS: [i64; 40] = [
    clash::MAX_ARRIVALS as i64,
    clash::MAX_GARRISONS as i64,
    clash::NEUTRAL as i64,
    clash::REFUND_RATIO as i64,
    clash::RETREAT_MAX_BPS as i64,
    clash::MAX_DAMAGE_PRODUCT_BPS as i64,
    clash::PEACE_LEAD_BELLS as i64,
    clash::WAR_HORN_BELLS as i64,
    clash::Occupancy::STORAGE as i64,
    host::MAX_HOST_TROOPS as i64,
    host::MIN_HOST_TROOPS as i64,
    host::DESTROYED_BELOW as i64,
    host::STAMINA_CAP as i64,
    host::ENGAGE_STAMINA as i64,
    host::BATTLE_COOLDOWN_BELLS as i64,
    host::HEX_HOST_CAP as i64,
    host::OWNER_HEX_SLOTS as i64,
    host::PROVINCE_HOST_CAP as i64,
    host::FACTION_RESIDENT_CAP as i64,
    host::ROUT_LOSS_BPS as i64,
    holding::MAX_WALLS as i64,
    holding::MAX_PRODUCTION_PER_HOUR,
    holding::MAX_UPKEEP_PER_HOUR,
    holding::MAX_DUPLICATES as i64,
    holding::DORMANT_AFTER,
    holding::SHIELD_SECS,
    siege::SIEGE_BASE_BELLS as i64,
    siege::WALL_POINTS_PER_BELL as i64,
    siege::VIGIL_SECS,
    siege::VIGIL_CHANGE_INTERVAL,
    siege::VIGIL_NOTICE,
    siege::AUTO_REINFORCE_MAX_BPS as i64,
    siege::RAID_MAX_BPS as i64,
    travel::FLAT_HEX_SECS as i64,
    travel::ROUGH_HEX_SECS as i64,
    travel::ROAD_BPS as i64,
    travel::CAVALRY_BPS as i64,
    travel::SUPPLY_RANGE_PROVINCES as i64,
    travel::MARCH_STAMINA_BASE as i64,
    travel::MARCH_STAMINA_PER_HEX as i64,
];

/// The bytes `RULESET_HASH` is the sha256 of (contract §3.2): the domain,
/// every kernel version constant (name-length-prefixed, one per module),
/// the digest of the v9 combat tables the clash reuses
/// (`clash::frontier_ruleset()`), the catalog tables, the camp and explore
/// constants, the seal's pinned limits, the office default, the doctrine
/// table, the stance damage table and [`KERNEL_CONSTANTS`]. It does not
/// bind code: an algorithm change that keeps every constant must bump its
/// module's version. `frontier-abi`'s build step hashes it into the program;
/// the verifier and the web client compare against it.
pub fn ruleset_hash_input() -> alloc::vec::Vec<u8> {
    let mut out = alloc::vec::Vec::with_capacity(1_024);
    out.extend_from_slice(RULESET_DOMAIN);
    out.push(KERNEL_VERSIONS.len() as u8);
    for (name, v) in KERNEL_VERSIONS {
        out.push(name.len() as u8);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&clash::frontier_ruleset().hash());
    catalog::write_tables(&mut out);
    for x in [
        camp::CAMP_TROOPS_MIN,
        camp::CAMP_TROOPS_SPREAD,
        camp::CAMP_FIRST_RING,
    ] {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out.push(explore::EXPLORE_FLOOR);
    out.extend_from_slice(&seal::RETREAT_MAX_BPS.to_le_bytes());
    out.push(seal::PLAIN_VERSION);
    out.extend_from_slice(&(seal::PLAIN_LEN as u16).to_le_bytes());
    out.extend_from_slice(&(seal::SEAL_LEN as u16).to_le_bytes());
    out.extend_from_slice(&office::OFFICE_TERMS_PER_WALLET_DEFAULT.to_le_bytes());
    // v1.2 of the input (integ-W1): the doctrine table, the stance damage
    // table and the outcome kernels' constants.
    doctrine::write_table(&mut out);
    let postures = [
        stance::Posture::Stance(stance::Stance::Hold),
        stance::Posture::Stance(stance::Stance::Assault),
        stance::Posture::Stance(stance::Stance::Flank),
        stance::Posture::Stance(stance::Stance::Brace),
        stance::Posture::Disarray,
    ];
    for a in postures {
        for d in postures {
            out.extend_from_slice(&stance::damage_bps(a, d).to_le_bytes());
        }
    }
    out.push(KERNEL_CONSTANTS.len() as u8);
    for x in KERNEL_CONSTANTS {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// `RULESET_HASH = sha256(ruleset_hash_input())`.
pub fn ruleset_hash() -> [u8; 32] {
    crate::hash::sha256(&[&ruleset_hash_input()])
}
