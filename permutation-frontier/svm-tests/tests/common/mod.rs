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

/// `g01_loaded_limit_*` (§13.1, I-45): sends `ixs` at `L(ix)` at the
/// chain's deployed programdata length (must load and land, on a fork) and
/// one page below the tight limit (must fail `MaxLoadedAccountsDataSize
/// Exceeded` with the fee charged); `L(ix)` is the tight limit or one page
/// above it (the formula over-counts by less than a page, so `ixs` must be
/// the kind's worst account set). Returns the need.
#[track_caller]
pub fn loaded_check(
    c: &Chain,
    ix: frontier_abi::tags::Ix,
    ixs: &[permutation_frontier_svm_tests::Instruction],
    signers: &[&Keypair],
) -> u64 {
    use permutation_frontier_svm_tests::chain::{
        assert_loaded_exceeded, expect_lands, Profile, PAGE,
    };
    let pd = c.programdata_len();
    let l = frontier_abi::budgets::loaded_limit_for(ix, pd);
    let p = Profile::ladder(ix, pd).with_loaded(l);
    let t = c.transaction(&p, ixs, signers);
    let need = c.loaded_size(&t.message);
    let tight = (need.div_ceil(PAGE as u64) * PAGE as u64) as u32;
    println!(
        "g01 L({}) = {l} B at programdata {pd} B: need {need} B, tight {tight} B, slack {} B",
        ix.name(),
        l as u64 - need.min(l as u64)
    );
    assert!(
        need <= l as u64,
        "{}: need {need} B > L(kind) {l} B",
        ix.name()
    );
    let mut f = c.fork();
    let landed = expect_lands(f.send_with(&p, ixs, signers), ix.name());
    assert_eq!(landed.loaded, need);
    let mut f = c.fork();
    assert_loaded_exceeded(f.send_with(&p.with_loaded(tight - PAGE), ixs, signers));
    assert!(
        l == tight || l == tight + PAGE,
        "{}: L(kind) {l} B is not within a page of the tight {tight} B (not the worst set?)",
        ix.name()
    );
    need
}
