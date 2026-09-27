//! Helpers shared by the gate test files (`mod common;`).
#![allow(dead_code)]

use permutation_frontier_svm_tests::chain::{Chain, Landed};
use permutation_frontier_svm_tests::world::World;
use permutation_frontier_svm_tests::{Address, Keypair, Signer};

/// A running season (id 1) on the release binary.
pub fn release() -> (Chain, World) {
    let mut c = Chain::release();
    let w = World::running(&mut c, 1);
    (c, w)
}

/// A running season (id 1) on the test-beacon binary (any round, I-53).
pub fn test_beacon() -> (Chain, World) {
    let mut c = Chain::test_beacon();
    let w = World::running(&mut c, 1);
    (c, w)
}

/// Pre-funding amounts of §13.2: rent and 10× rent (and one lamport, the
/// smallest amount that breaks `CreateAccount`).
pub fn prefunds(rent: u64) -> [u64; 3] {
    [1, rent, 10 * rent]
}

/// What `payer` paid in the landed transaction `l`, fee excluded.
pub fn paid(before: u64, c: &Chain, payer: &Keypair, l: &Landed) -> u64 {
    before - c.lamports(&payer.pubkey()) - l.fee
}

/// §4.2: the payer tops a pre-funded target up to `rent + escrow` — or
/// pays the rent shortfall plus the whole escrow; both keep the target at
/// `rent + escrow` or more and never charge more than the shortfall plus
/// the escrow. Asserts one of the two.
#[track_caller]
pub fn assert_shortfall_only(what: &str, paid: u64, pre: u64, rent: u64, escrow: u64) {
    let topped = (rent + escrow).saturating_sub(pre);
    let separate = rent.saturating_sub(pre) + escrow;
    assert!(
        paid == topped || paid == separate,
        "{what}: pre-funded {pre}, rent {rent}, escrow {escrow}: paid {paid} (want {topped} or {separate})"
    );
}

/// Asserts `k` is a program account of `size` bytes with `magic` for `season_id`.
#[track_caller]
pub fn assert_program_account(c: &Chain, k: &Address, magic: [u8; 8], size: usize, season_id: u64) {
    let a = c.account(k).unwrap_or_else(|| panic!("{k} exists"));
    assert_eq!(a.owner, c.program, "{k}: owner");
    assert_eq!(a.data.len(), size, "{k}: size");
    assert_eq!(a.data[..8], magic, "{k}: magic");
    assert_eq!(
        u64::from_le_bytes(a.data[8..16].try_into().unwrap()),
        season_id,
        "{k}: season id"
    );
    assert!(a.lamports >= c.rent(size), "{k}: rent-exempt");
}

/// A program-owned copy of `from`'s data at a fresh address.
pub fn copy_to_fresh(c: &mut Chain, from: &Address, label: &[u8]) -> Address {
    let k = Address::new_from_array(permutation_frontier_svm_tests::sha256(&[b"fresh", label]));
    let d = c.data(from);
    c.put_program_account(k, d);
    k
}
