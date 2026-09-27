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
/// (append only).
pub const KERNEL_VERSIONS: [(&str, u16); 9] = [
    ("frontier", RULES_VERSION_FRONTIER),
    ("beacon", beacon::BEACON_VERSION),
    ("addr", addr::ADDR_VERSION),
    ("seal", seal::SEAL_VERSION),
    ("fees", fees::FEES_VERSION),
    ("office", office::OFFICE_VERSION),
    ("camp", camp::CAMP_VERSION),
    ("explore", explore::EXPLORE_VERSION),
    ("catalog", catalog::CATALOG_VERSION),
];

/// The bytes `RULESET_HASH` is the sha256 of (contract §3.2): the domain,
/// every kernel version constant (name-length-prefixed), the digest of
/// the v9 combat tables the clash reuses (`clash::frontier_ruleset()`),
/// the catalog tables, the camp and explore constants and the seal's
/// pinned limits. `frontier-abi`'s build step hashes it into the program;
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
    out
}

/// `RULESET_HASH = sha256(ruleset_hash_input())`.
pub fn ruleset_hash() -> [u8; 32] {
    crate::hash::sha256(&[&ruleset_hash_input()])
}
