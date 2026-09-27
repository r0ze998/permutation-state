//! Solvency (WP12): FinishSeason sets aside exactly what the vault holds
//! (`outstanding`), rounds treasury refunds to exact splits of the uniform
//! deposit, voids a season whose world did not conserve USDC, and Claim /
//! WithdrawOps never pay past what was set aside. A season of the previous
//! layout (`PSSEASN7`) still claims.

use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::payout::claim_amount;
use permutation_chain_svm_tests::state::*;
use permutation_chain_svm_tests::*;
use solana_signer::Signer;

/// Four members, two per nation, each depositing `dep` (the season's
/// uniform deposit), seated and open.
fn uniform(c: &mut Chain, dep: u64) -> SeasonFx {
    let mut s = SeasonFx::create(c, Params::default());
    c.edit::<Season>(&s.season, |x| x.deposit = dep);
    for i in 0..4 {
        s.register(c, (i % 2) as u16, dep);
    }
    s.genesis(c);
    s.seat_and_open(c);
    s
}

/// Moves `amount` of treasury USDC from nation `from` to nation `to` in the
/// stored world (conserved: a trade or a contract would do the same).
fn shift(c: &mut Chain, s: &SeasonFx, from: usize, to: usize, amount: u64) {
    let chunks = s.chunks.clone();
    c.with_world(&chunks, |w| {
        let mut st = w.read_world().unwrap();
        st.civs[from].usdc -= amount;
        st.civs[to].usdc += amount;
        w.write_world(&st).unwrap();
    });
}

/// The devnet case of an odd treasury split over two depositors: every
/// member claims and operations withdraws, and the vault ends at exactly 0.
#[test]
fn a_full_settlement_empties_the_vault() {
    let mut c = Chain::new();
    let dep = 2_833_333;
    let s = uniform(&mut c, dep);
    s.fast_forward_to_end(&mut c);
    shift(&mut c, &s, 1, 0, 1); // nation 0: 5,666,667 over two deposits
    let anyone = c.funded();
    let vault_in = c.balance(&s.vault);
    c.send(vec![s.finish_ix(false)], &[&anyone])
        .expect("FinishSeason");
    let season = s.season(&c);
    assert!(!season.voided);
    assert_eq!(season.outstanding, vault_in);
    assert_eq!(season.treasury_final[0] % 2, 0, "an exact split");
    let out = s.settle(&mut c);
    assert_eq!((out.left, s.season(&c).outstanding), (0, 0));
    assert_eq!(out.claims + out.ops, vault_in);
}

/// A world whose USDC does not add up to the deposits (u128) voids the
/// season: every member gets back its fee and deposit, the operator its
/// escrow, and the vault ends at 0.
#[test]
fn voided_season_refunds_paid_in() {
    let mut c = Chain::new();
    let (bounty, bond) = (1_000_000, 4_000_000);
    // One deposit per season (WP12), paid by the AI member too.
    let p = Params {
        deposit: 2_000_000,
        ..Params::default()
    };
    let mut s = SeasonFx::create_ai(&mut c, p, &[0], bounty, bond);
    s.register_ai(&mut c, 0);
    for i in 0..3 {
        s.register(&mut c, (i % 2) as u16, s.p.deposit);
    }
    s.genesis(&mut c);
    s.seat_and_open(&mut c);
    s.fast_forward_to_end(&mut c);
    // One base unit that nobody deposited.
    let chunks = s.chunks.clone();
    c.with_world(&chunks, |w| {
        let mut st = w.read_world().unwrap();
        st.civs[1].usdc += 1;
        w.write_world(&st).unwrap();
    });
    c.advance(ROSTER_GRACE_SECONDS);
    let anyone = c.funded();
    let vault_in = c.balance(&s.vault);
    let l = c
        .send(vec![s.finish_ix(false)], &[&anyone])
        .expect("FinishSeason");
    assert!(l.logs.iter().any(|x| x.contains("VOIDED")), "{:?}", l.logs);
    let season = s.season(&c);
    assert!(season.voided);
    assert_eq!(season.outstanding, vault_in);
    for i in 0..s.members.len() {
        let m = s.member(&c, i);
        assert_eq!(claim_amount(&season, &m), s.p.fee + m.shares);
    }
    let out = s.settle(&mut c);
    // `genesis` topped the bond up to its floor (PostBond, WP09).
    assert_eq!(out.ops, bounty + season.bond, "the operator's escrow");
    assert!(season.bond >= bond);
    assert_eq!((out.left, s.season(&c).outstanding), (0, 0));
}

/// A tick that once left USDC unconserved (`WorldMeta::usdc_broken`, set by
/// ResolveTick and never cleared) voids the season even when the final
/// world adds up.
#[test]
fn a_broken_tick_voids_the_season() {
    let mut c = Chain::new();
    let s = uniform(&mut c, 1_000_000);
    s.fast_forward_to_end(&mut c);
    let chunks = s.chunks.clone();
    c.with_world(&chunks, |w| {
        let mut m = w.meta().unwrap();
        m.usdc_broken = true;
        w.set_meta(&m).unwrap();
    });
    let anyone = c.funded();
    c.send(vec![s.finish_ix(false)], &[&anyone])
        .expect("FinishSeason");
    assert!(s.season(&c).voided);
    let out = s.settle(&mut c);
    assert_eq!(out.left, 0);
}

/// A claim above what is left of `outstanding` is refused.
#[test]
fn claim_refuses_more_than_outstanding() {
    let mut c = Chain::new();
    let s = uniform(&mut c, 1_000_000);
    s.fast_forward_to_end(&mut c);
    let anyone = c.funded();
    c.send(vec![s.finish_ix(false)], &[&anyone])
        .expect("FinishSeason");
    let w = s.members[1].wallet.insecure_clone();
    let owed = claim_amount(&s.season(&c), &s.member(&c, 1));
    c.edit::<Season>(&s.season, |x| x.outstanding = owed - 1);
    assert_err(
        c.send(vec![s.claim_ix(1, &w.pubkey(), &s.members[1].token)], &[&w]),
        E::Insolvent,
    );
}

/// A season finalized by the previous program (`PSSEASN7`: every field up
/// to `bounty_paid`, a zero tail) still claims by its own rule
/// (`refund_base` absent: over the deposits) and keeps no `outstanding`
/// (WP12 C11, WP15 test 14: the append-only tail reads as zero).
#[test]
fn legacy_v7_season_still_claims() {
    let mut c = Chain::new();
    let s = uniform(&mut c, 1_000_000);
    s.fast_forward_to_end(&mut c);
    let anyone = c.funded();
    c.send(vec![s.finish_ix(false)], &[&anyone])
        .expect("FinishSeason");
    let v8 = s.season(&c);
    let mut v7 = v8.clone();
    v7.magic = LEGACY_SEASON_MAGIC;
    (v7.delegated, v7.roster_blind) = (0, [0; 32]);
    (v7.refund_base, v7.refund_in_payout) = (vec![], vec![]);
    (
        v7.seed_state,
        v7.seed_oracle,
        v7.seed_requested_at,
        v7.seed_requests,
    ) = (0, [0; 32], 0, 0);
    (v7.deposit, v7.outstanding, v7.voided) = (0, 0, false);
    (v7.start_by, v7.stage_at, v7.rolled_back, v7.aborted_from) = (0, 0, 0, 0);
    v7.validator = [0; 32];
    (
        v7.rules_version,
        v7.rules_hash,
        v7.logic_version,
        v7.created_slot,
    ) = (0, [0; 32], 0, 0);
    let mut data = vec![0u8; SEASON_SPACE];
    let bytes = borsh::to_vec(&v7).unwrap();
    data[..bytes.len()].copy_from_slice(&bytes);
    c.set_data(&s.season, data.clone());
    let w = s.members[1].wallet.insecure_clone();
    let t = s.members[1].token;
    let owed = claim_amount(&v7, &s.member(&c, 1));
    assert!(owed > 0);
    c.send(vec![s.claim_ix(1, &w.pubkey(), &t)], &[&w])
        .expect("Claim of a v7 season");
    assert_eq!(c.balance(&t), owed);
    assert_eq!(c.data(&s.season), data, "the v7 account is left as it was");
    let admin = s.admin.insecure_clone();
    let dest = c.token_account(&s.mint, &admin.pubkey(), 0);
    c.send(vec![s.withdraw_ix(&admin.pubkey(), &dest)], &[&admin])
        .expect("WithdrawOps of a v7 season");
    assert_eq!(c.balance(&dest), v7.ops);
    let after: Season = c.load(&s.season);
    assert_eq!(after.magic, LEGACY_SEASON_MAGIC);
    assert!(after.ops_withdrawn);
}

/// WP12 §6.3(9): FinishSeason's three paths (normal, voided by the sticky
/// flag, voided by books that do not balance) each fit the Heavy profile.
/// The capacity gate at the member cap is unit T's (`budget.rs`).
#[test]
fn finish_season_cu_and_heap_in_all_three_paths() {
    let mut c = Chain::new();
    let s = SeasonFx::running(
        &mut c,
        Params {
            nations: 6,
            ..Params::default()
        },
        12,
    );
    for _ in 0..2 {
        s.play_tick(&mut c);
    }
    s.fast_forward_to_end(&mut c);
    let anyone = c.funded();
    let normal = c
        .need(&[s.finish_ix(false)], &[&anyone])
        .expect("FinishSeason, normal");
    assert_fits("FinishSeason, normal", normal, Budget::Heavy);
    let mut flagged = c.fork();
    let chunks = s.chunks.clone();
    flagged.with_world(&chunks, |w| {
        let mut m = w.meta().unwrap();
        m.usdc_broken = true;
        w.set_meta(&m).unwrap();
    });
    let broken = flagged
        .need(&[s.finish_ix(false)], &[&anyone])
        .expect("FinishSeason, usdc_broken");
    assert_fits("FinishSeason, voided (flag)", broken, Budget::Heavy);
    let mut unbalanced = c.fork();
    unbalanced.edit::<Season>(&s.season, |x| x.pool += 1);
    let off = unbalanced
        .need(&[s.finish_ix(false)], &[&anyone])
        .expect("FinishSeason, unbalanced");
    assert_fits("FinishSeason, voided (books)", off, Budget::Heavy);
    unbalanced
        .send(vec![s.finish_ix(false)], &[&anyone])
        .expect("FinishSeason");
    assert!(s.season(&unbalanced).voided);
    println!("FinishSeason: normal {normal:?}, flag {broken:?}, books {off:?}");
}
