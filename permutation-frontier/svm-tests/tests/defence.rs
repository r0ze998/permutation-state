//! ClaimDefence (0x70), W4-B, on the test-beacon build: Reveal-only
//! eligibility (I-21) with the claim grace (I-52), the per-(bell, region)
//! cap inside a claim and the per-keeper-day cap, partial payment from an
//! empty pool; G2 (pre-funded DefenceClaim), G3 forgeries, G1 budget, G13
//! codes.
//!
//! Crafted: ArrivalSlots written as a Reveal leaves them (`World::
//! craft_slot`, then their beneficiary and `ev_*` evidence set as the
//! keeper's late Reveal would), so six slots of one bell-region need no
//! six marches; THE anchors are posted by the program. The claim after a
//! real Reveal and SettleTransit is `transit::g12_claim_after_settle_*`.

mod common;

use common::{assert_program_account, assert_shortfall_only, paid, prefunds};
use frontier_abi::layout::beacon::defence_claim as DCL;
use frontier_abi::layout::clash::arrival_slot as AS;
use frontier_abi::layout::world::defence_pool as DP;
use frontier_abi::log::Kind;
use frontier_abi::tags::Ix;
use permutation_frontier_svm_tests::budget::{assert_within, ceilings};
use permutation_frontier_svm_tests::chain::{assert_code, expect_lands, with_account, Chain};
use permutation_frontier_svm_tests::ix::defence::{at, claim_defence, ClaimSlot};
use permutation_frontier_svm_tests::records::{self, le};
use permutation_frontier_svm_tests::world::World;
use permutation_frontier_svm_tests::{Address, FrontierError as E, Instruction, Keypair, Signer};
use permutation_rules::frontier::fees::{self, DefenceParams, Evidence};

/// The destination of the crafted slots and its region.
const DEST: (i32, i32) = (3, -1);
const BELL: u32 = 12;
/// A keeper Reveal's evidence: 1.4M CU at 20,000 µlamports/CU (28,000
/// lamports of priority), 1 MiB loaded, 5 slots after THE anchor.
const PRICE: u64 = 20_000;
const LIMIT: u32 = 1_400_000;
const LOADED: u32 = 1 << 20;
const LATE: u64 = 5;

struct Scene {
    c: Chain,
    w: World,
    keeper: Keypair,
    anchor_slot: u64,
}

fn scene() -> Scene {
    let (mut c, w) = common::test_beacon();
    let region = World::region(DEST.0, DEST.1);
    expect_lands(w.post_anchor(&mut c, BELL, region), "PostAnchor");
    let d = c.data(&w.a.anchor(BELL, region));
    let anchor_slot = le(&d[frontier_abi::layout::beacon::bell_anchor::SLOT..][..8]);
    let keeper = c.funded(b"w4b-claim-keeper", 1);
    Scene {
        c,
        w,
        keeper,
        anchor_slot,
    }
}

impl Scene {
    /// Slot `(DEST, BELL, faction, i)` as the keeper's late Reveal leaves it.
    fn slot(&mut self, faction: u8, i: u8, price: u64, late: u64) -> Address {
        let k = self.w.craft_slot(
            &mut self.c,
            DEST,
            BELL,
            faction,
            i,
            0x7000 + (faction as u64) * 8 + i as u64,
            0x1000 + i as u64,
            500_000,
        );
        let ben = self.keeper.pubkey();
        let ev_slot = self.anchor_slot + late;
        self.c.edit(&k, |d| {
            d[AS::BENEFICIARY..AS::BENEFICIARY + 32].copy_from_slice(ben.as_ref());
            d[AS::EV_SLOT..AS::EV_SLOT + 8].copy_from_slice(&ev_slot.to_le_bytes());
            d[AS::EV_PRICE..AS::EV_PRICE + 8].copy_from_slice(&price.to_le_bytes());
            d[AS::EV_LIMIT..AS::EV_LIMIT + 4].copy_from_slice(&LIMIT.to_le_bytes());
            d[AS::EV_LOADED..AS::EV_LOADED + 4].copy_from_slice(&LOADED.to_le_bytes());
        });
        k
    }
    fn day(&self) -> u32 {
        self.w.bell(&self.c) / 144
    }
    fn claim(&self, slots: &[(u8, u8)]) -> Instruction {
        let v: Vec<ClaimSlot> = slots
            .iter()
            .map(|&(faction, i)| ClaimSlot {
                p: DEST.0,
                q: DEST.1,
                bell: BELL,
                faction,
                i,
            })
            .collect();
        claim_defence(&self.w.a, self.keeper.pubkey(), self.day(), &v)
    }
    fn send(&mut self, ix: Instruction) -> permutation_frontier_svm_tests::chain::SendResult {
        let k = self.keeper.insecure_clone();
        self.c.send(&[ix], &[&k])
    }
    fn refund(&self) -> u64 {
        let tip_min = self.w.tip_min(&self.c);
        fees::defence_refund(
            &Evidence {
                price_micro: PRICE,
                limit: LIMIT,
                loaded: LOADED,
                created_day: false,
            },
            &DefenceParams {
                defence_cap_milli: 2_000,
                tip_min,
            },
        )
    }
    fn set_pool(&mut self, off: usize, v: u64) {
        let k = self.w.a.defence_pool();
        self.c
            .edit(&k, |d| d[off..off + 8].copy_from_slice(&v.to_le_bytes()));
    }
}

/// Six late, overpaid keeper reveals of one bell-region: the refunds are
/// summed under the per-(bell, region) cap, then the per-keeper-day cap
/// across claims, and an empty pool pays partially (logged).
#[test]
fn g12_claim_defence_caps_and_partial_pool() {
    let mut s = scene();
    let refund = s.refund();
    assert_eq!(refund, 28_000 - (s.w.tip_min(&s.c) - 2_500));
    // Caps: 3 refunds per bell-region; 4 refunds per keeper-day.
    s.set_pool(DP::PER_BELL_REGION_CAP, 3 * refund);
    s.set_pool(DP::PER_KEEPER_DAY_CAP, 4 * refund);
    let six: Vec<(u8, u8)> = (0..6u8).map(|k| (k / 4, k % 4)).collect();
    for &(f, i) in &six {
        s.slot(f, i, PRICE, LATE);
    }
    let pool = s.w.a.defence_pool();
    let (p0, k0) = (s.c.lamports(&pool), s.c.lamports(&s.keeper.pubkey()));
    let claim_key = s.w.a.defence_claim(&s.keeper.pubkey(), s.day());
    let l = expect_lands(s.send(s.claim(&six)), "s.claim(");
    let r = records::one(&l.logs, Kind::DEFENCE_CLAIM);
    assert_eq!(r.u64("amount"), 3 * refund, "the bell-region cap binds");
    assert_eq!(r.u64("slots"), 6);
    assert_eq!(r.u64("partial"), 0);
    let rent = s.c.rent(DCL::SIZE);
    assert_eq!(s.c.lamports(&pool), p0 - 3 * refund);
    assert_eq!(
        s.c.lamports(&s.keeper.pubkey()) + l.fee + rent,
        k0 + 3 * refund
    );
    for &(f, i) in &six {
        let d = s.c.data(&s.w.a.arrival_slot(DEST.0, DEST.1, BELL, f, i));
        assert_eq!(d[AS::CLAIMED], 1);
    }
    let cd = s.c.data(&claim_key);
    assert_eq!(le(&cd[DCL::CLAIMED..DCL::CLAIMED + 8]), 3 * refund);
    assert_eq!(le(&cd[DCL::COUNT..DCL::COUNT + 4]), 6);
    let pd = s.c.data(&pool);
    assert_eq!(le(&pd[DP::PAID_TOTAL..DP::PAID_TOTAL + 8]), 3 * refund);
    assert_eq!(le(&pd[DP::CLAIMS..DP::CLAIMS + 8]), 6);
    // Another bell-region the same day: the day cap leaves one refund.
    let region = World::region(DEST.0, DEST.1);
    let bell2 = BELL + 1;
    expect_lands(s.w.post_anchor(&mut s.c, bell2, region), "PostAnchor");
    let a2 = le(&s.c.data(&s.w.a.anchor(bell2, region))
        [frontier_abi::layout::beacon::bell_anchor::SLOT..][..8]);
    let k2 =
        s.w.craft_slot(&mut s.c, DEST, bell2, 0, 0, 0x9000, 0x2000, 500_000);
    let ben = s.keeper.pubkey();
    s.c.edit(&k2, |d| {
        d[AS::BENEFICIARY..AS::BENEFICIARY + 32].copy_from_slice(ben.as_ref());
        d[AS::EV_SLOT..AS::EV_SLOT + 8].copy_from_slice(&(a2 + LATE).to_le_bytes());
        d[AS::EV_PRICE..AS::EV_PRICE + 8].copy_from_slice(&PRICE.to_le_bytes());
        d[AS::EV_LIMIT..AS::EV_LIMIT + 4].copy_from_slice(&LIMIT.to_le_bytes());
        d[AS::EV_LOADED..AS::EV_LOADED + 4].copy_from_slice(&LOADED.to_le_bytes());
    });
    let day = s.day();
    let one = claim_defence(
        &s.w.a,
        s.keeper.pubkey(),
        day,
        &[ClaimSlot {
            p: DEST.0,
            q: DEST.1,
            bell: bell2,
            faction: 0,
            i: 0,
        }],
    );
    // An almost empty pool: partial.
    let mut f = s.c.fork();
    let keep = f.rent(DP::SIZE) + 1_000;
    let mut acc = f.account(&pool).unwrap();
    acc.lamports = keep;
    f.put(pool, acc.owner, acc.data, keep);
    let k = s.keeper.insecure_clone();
    let l = expect_lands(f.send(std::slice::from_ref(&one), &[&k]), "partial claim");
    let r = records::one(&l.logs, Kind::DEFENCE_CLAIM);
    assert_eq!((r.u64("amount"), r.u64("partial")), (1_000, 1));
    let l = expect_lands(s.send(one), "day-capped claim");
    assert_eq!(
        records::one(&l.logs, Kind::DEFENCE_CLAIM).u64("amount"),
        refund
    );
}

/// ClaimDefence's refusals (G13) and forgeries (G3), in the order of §5.12.
#[test]
fn g13_claim_defence_refusals() {
    let mut s = scene();
    s.slot(0, 0, PRICE, LATE);
    s.slot(0, 1, PRICE, 1); // on time
    s.slot(0, 2, 0, LATE); // at the tip level: no refund
    let ix = s.claim(&[(0, 0)]);
    // BadData: another day; n = 0 (decode).
    let mut bad = ix.clone();
    bad.data[1..5].copy_from_slice(&(s.day() + 1).to_le_bytes());
    assert_code(s.send(bad), E::BadData);
    // NotEligible: on time, cheap, another beneficiary, or claimed twice.
    assert_code(s.send(s.claim(&[(0, 1)])), E::NotEligible);
    assert_code(s.send(s.claim(&[(0, 2)])), E::NotEligible);
    assert_code(s.send(s.claim(&[(0, 0), (0, 0)])), E::NotEligible);
    let other = s.c.funded(b"w4b-other-keeper", 1);
    let theirs = claim_defence(
        &s.w.a,
        other.pubkey(),
        s.day(),
        &[ClaimSlot {
            p: DEST.0,
            q: DEST.1,
            bell: BELL,
            faction: 0,
            i: 0,
        }],
    );
    assert_code(s.c.send(&[theirs], &[&other]), E::NotEligible);
    // Forgeries: a slot copied elsewhere (BadAddress), a non-canonical
    // claim or pool (BadAddress), a slot of another season (BadAccount).
    let slot = s.w.a.arrival_slot(DEST.0, DEST.1, BELL, 0, 0);
    let fresh = common::copy_to_fresh(&mut s.c, &slot, b"claim-slot");
    assert_code(
        s.send(with_account(ix.clone(), at::FIRST_SLOT, fresh)),
        E::BadAddress,
    );
    let wrong_claim = s.w.a.defence_claim(&other.pubkey(), s.day());
    assert_code(
        s.send(with_account(ix.clone(), at::CLAIM, wrong_claim)),
        E::BadAddress,
    );
    assert_code(
        s.send(with_account(ix.clone(), at::DPOOL, s.w.a.frontier())),
        E::BadAddress,
    );
    let mut f = s.c.fork();
    f.edit(&slot, |d| d[8] ^= 1);
    let k = s.keeper.insecure_clone();
    assert_code(f.send(std::slice::from_ref(&ix), &[&k]), E::BadAccount);
    // NoAnchor: THE anchor of another bell is absent.
    let k3 =
        s.w.craft_slot(&mut s.c, DEST, BELL + 5, 0, 0, 0x9100, 0x3000, 500_000);
    let ben = s.keeper.pubkey();
    s.c.edit(&k3, |d| {
        d[AS::BENEFICIARY..AS::BENEFICIARY + 32].copy_from_slice(ben.as_ref())
    });
    let no_anchor = claim_defence(
        &s.w.a,
        s.keeper.pubkey(),
        s.day(),
        &[ClaimSlot {
            p: DEST.0,
            q: DEST.1,
            bell: BELL + 5,
            faction: 0,
            i: 0,
        }],
    );
    assert_code(s.send(no_anchor), E::NoAnchor);
    // WrongStatus.
    let mut f = s.c.fork();
    s.w.craft_status(&mut f, frontier_abi::layout::world::season::STATUS_ABORTED);
    assert_code(f.send(std::slice::from_ref(&ix), &[&k]), E::WrongStatus);
    // W5-A (G13): a slot account missing for the data's slot count
    // (TooManyAccounts); the keeper position not signing, another key
    // paying the fee (Auth).
    let mut short = ix.clone();
    short.accounts.pop();
    assert_code(s.c.fork().send(&[short], &[&k]), E::TooManyAccounts);
    let payer = s.c.funded(b"w5a-claim-fee-payer", 1);
    let mut unsigned = ix.clone();
    unsigned.accounts[0].is_signer = false;
    assert_code(s.c.fork().send(&[unsigned], &[&payer]), E::Auth);
    // WindowClosed: past close + 6 bells.
    let region = World::region(DEST.0, DEST.1);
    let a = s.w.anchor_a(&s.c, BELL, region).unwrap();
    let close = permutation_rules::frontier::beacon::reveal_close(a, s.w.window(&s.c, BELL));
    let mut f = s.c.fork();
    f.set_time(close + 6 * 600);
    let late_ix = {
        let mut i2 = ix.clone();
        let day = (f.now - s.w.genesis_ts()) as u32 / 600 / 144;
        i2.data[1..5].copy_from_slice(&day.to_le_bytes());
        with_account(i2, at::CLAIM, s.w.a.defence_claim(&s.keeper.pubkey(), day))
    };
    assert_code(f.send(&[late_ix], &[&k]), E::WindowClosed);
    // The eligible one lands.
    expect_lands(s.send(ix), "s.claim(");
}

/// G2 (§13.2): the DefenceClaim created on a pre-funded address.
#[test]
fn g02_prefund_defence_claim() {
    let mut base = scene();
    base.slot(0, 0, PRICE, LATE);
    let rent = base.c.rent(DCL::SIZE);
    let key = base.w.a.defence_claim(&base.keeper.pubkey(), base.day());
    for pre in prefunds(rent) {
        let mut c = base.c.fork();
        c.prefund(&key, pre);
        let before = c.lamports(&base.keeper.pubkey());
        let l = expect_lands(
            c.send(&[base.claim(&[(0, 0)])], &[&base.keeper]),
            "ClaimDefence on a pre-funded claim",
        );
        let refund = records::one(&l.logs, Kind::DEFENCE_CLAIM).u64("amount");
        // The keeper paid the rent shortfall and received the refund.
        let spent = paid(before + refund, &c, &base.keeper, &l);
        assert_shortfall_only("DefenceClaim", spent, pre, rent, 0);
        assert_program_account(&c, &key, DCL::MAGIC, DCL::SIZE, base.w.id);
    }
}

/// G1 (§13.1): ClaimDefence with 6 slots and binding caps — the worst
/// case: 6 slots of 6 bells (6 anchors to derive and read), the per-keeper
/// day cap binding; and 6 slots of one bell-region with its cap binding.
#[test]
fn g01_budget_w4b_claim_defence() {
    let mut s = scene();
    let refund = s.refund();
    s.set_pool(DP::PER_BELL_REGION_CAP, 3 * refund);
    let six: Vec<(u8, u8)> = (0..6u8).map(|k| (k / 4, k % 4)).collect();
    for &(f, i) in &six {
        s.slot(f, i, PRICE, LATE);
    }
    let need =
        s.c.measure(&[s.claim(&six)], &[&s.keeper])
            .expect("measures");
    assert_within(
        "ClaimDefence 6 slots of one bell-region, its cap binding",
        &need,
        &ceilings(Ix::ClaimDefence, 0, s.c.programdata_len()),
    );
    // Six bells.
    let mut s = scene();
    s.set_pool(DP::PER_KEEPER_DAY_CAP, 2 * refund);
    let region = World::region(DEST.0, DEST.1);
    let ben = s.keeper.pubkey();
    let mut slots = vec![];
    for k in 0..6u32 {
        let bell = BELL + k;
        if s.w.anchor_a(&s.c, bell, region).is_none() {
            expect_lands(s.w.post_anchor(&mut s.c, bell, region), "PostAnchor");
        }
        let a = le(&s.c.data(&s.w.a.anchor(bell, region))
            [frontier_abi::layout::beacon::bell_anchor::SLOT..][..8]);
        let key = s.w.craft_slot(
            &mut s.c,
            DEST,
            bell,
            0,
            0,
            0x9000 + k as u64,
            0x2000 + k as u64,
            500_000,
        );
        s.c.edit(&key, |d| {
            d[AS::BENEFICIARY..AS::BENEFICIARY + 32].copy_from_slice(ben.as_ref());
            d[AS::EV_SLOT..AS::EV_SLOT + 8].copy_from_slice(&(a + LATE).to_le_bytes());
            d[AS::EV_PRICE..AS::EV_PRICE + 8].copy_from_slice(&PRICE.to_le_bytes());
            d[AS::EV_LIMIT..AS::EV_LIMIT + 4].copy_from_slice(&LIMIT.to_le_bytes());
            d[AS::EV_LOADED..AS::EV_LOADED + 4].copy_from_slice(&LOADED.to_le_bytes());
        });
        slots.push(ClaimSlot {
            p: DEST.0,
            q: DEST.1,
            bell,
            faction: 0,
            i: 0,
        });
    }
    let ix = claim_defence(&s.w.a, s.keeper.pubkey(), s.day(), &slots);
    let need =
        s.c.measure(std::slice::from_ref(&ix), &[&s.keeper])
            .expect("measures");
    assert_within(
        "ClaimDefence 6 slots of 6 bells, day cap binding",
        &need,
        &ceilings(Ix::ClaimDefence, 0, s.c.programdata_len()),
    );
    let l = expect_lands(s.send(ix), "six bells");
    assert_eq!(
        records::one(&l.logs, Kind::DEFENCE_CLAIM).u64("amount"),
        2 * refund
    );
}

/// `g01_loaded_limit_*` (I-45) for ClaimDefence (6 slots).
#[test]
fn g01_loaded_limit_w4b_claim_defence() {
    use permutation_frontier_svm_tests::world::transit::loaded_check;
    let mut s = scene();
    let six: Vec<(u8, u8)> = (0..6u8).map(|k| (k / 4, k % 4)).collect();
    for &(f, i) in &six {
        s.slot(f, i, PRICE, LATE);
    }
    let k = s.keeper.insecure_clone();
    loaded_check(&s.c, Ix::ClaimDefence, &[s.claim(&six)], &[&k]);
}
