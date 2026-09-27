//! The escape hatches (WP14): Abort in every status with its deadlines and
//! the refunds after it (the vault ends at 0), the owner-side undelegation
//! escape (RequestUndelegation only once the season is done or past its
//! running deadline; RollbackUndelegation restoring the last finalized
//! state and blocking FinishSeason), and closing a done season's accounts.
//! The delegation program is the escape-aware stand-in (`dlp_escape`);
//! `DLP_SO` runs the two real-DLP tests.

use permutation_chain::lifecycle::{escrow, ops_after_abort, running_deadline};
use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::payout::claim_amount;
use permutation_chain_svm_tests::state::*;
use permutation_chain_svm_tests::*;
use solana_signer::Signer;

const BOUNTY: u64 = 1_000_003;
const BOND: u64 = 4_000_007;
const DAY: i64 = ABORT_GRACE_SECONDS;

/// Three AIs and nine people (deposits of 2 USDC), registered.
fn twelve(c: &mut Chain) -> SeasonFx {
    let mut s = SeasonFx::create_ai(c, Params::default(), &[0, 1, 0], BOUNTY, BOND);
    for i in 0..3 {
        s.register_ai(c, i);
    }
    for i in 0..9 {
        s.register(c, (i % 2) as u16, 2_000_000);
    }
    s
}

/// A season that starts its current stage now (the stage clock the program
/// keeps; set here so the tests do not depend on when the drivers ran).
fn stage_now(c: &mut Chain, s: &SeasonFx) -> i64 {
    let now = c.now;
    c.edit::<Season>(&s.season, |x| x.stage_at = now);
    now
}

/// Marks every target of the season delegated (as `Delegate` does) and
/// hands the accounts in `away` to the delegation program.
fn delegated(c: &mut Chain, s: &SeasonFx, away: &[u16]) {
    let n = s.p.nations;
    c.edit::<Season>(&s.season, |x| x.delegated = all_targets(n));
    for t in away {
        c.set_owner(&s.target(*t), addr(DLP));
    }
}

/// Every member claims and the admin withdraws; returns what the admin got.
fn refund_all(c: &mut Chain, s: &SeasonFx) -> u64 {
    let season = s.season(c);
    for i in 0..s.members.len() {
        let m = s.member(c, i);
        assert_eq!(
            claim_amount(&season, &m),
            crate_refund(&season, &m),
            "member {i}"
        );
    }
    let out = s.settle(c);
    assert_eq!(out.left, 0, "the vault ends at 0");
    assert_eq!(s.season(c).outstanding, 0);
    out.ops
}

/// What `Claim` pays after an abort (WP14 §3.3).
fn crate_refund(season: &Season, m: &MemberAccount) -> u64 {
    let share = if season.aborted_from != SeasonStatus::Registering as u8 {
        escrow(season) / season.member_count as u64
    } else {
        0
    };
    season.entry_fee + m.shares + share
}

/// WP14 test 1: the operator seats everyone and vanishes. After a day
/// anyone aborts; every member gets its fee, deposit and an equal share of
/// the escrow; operations only the remainder; the world and nations are
/// closed for their rent.
#[test]
fn a_frozen_seating_season_refunds_everyone() {
    let mut c = Chain::new();
    let s = twelve(&mut c);
    s.genesis(&mut c);
    s.seat_all(&mut c);
    let at = stage_now(&mut c, &s) + DAY;
    let admin = s.admin.insecure_clone();
    let outsider = c.funded();
    c.set_time(at - 1);
    assert_err(
        c.send(vec![s.abort_ix(&admin.pubkey())], &[&admin]),
        E::TooEarly,
    );
    let mut ix = s.abort_ix(&outsider.pubkey());
    ix.accounts[0].is_signer = false;
    assert_err(c.send(vec![ix], &[&admin]), E::MissingSignature);
    c.set_time(at);
    let vault_in = c.balance(&s.vault);
    let l = c
        .send(vec![s.abort_ix(&outsider.pubkey())], &[&outsider])
        .expect("Abort by an outsider");
    let season = s.season(&c);
    assert_eq!(season.status, SeasonStatus::Aborted);
    assert_eq!(season.aborted_from, SeasonStatus::Seating as u8);
    assert_eq!((season.stage_at, season.outstanding), (at, vault_in));
    assert_eq!(season.history_root, season.prev_history_root);
    assert_eq!(
        record(&l.logs, b"PS_ABORT")[1..],
        [
            s.p.id.to_le_bytes().to_vec(),
            vec![SeasonStatus::Seating as u8],
            at.to_le_bytes().to_vec()
        ]
    );
    assert_err(
        c.send(vec![s.abort_ix(&outsider.pubkey())], &[&outsider]),
        E::WrongStatus,
    );
    // A second claim is refused; operations get the rounding remainder.
    let w = s.members[4].wallet.insecure_clone();
    let t = s.members[4].token;
    let mut first = c.fork();
    first
        .send(vec![s.claim_ix(4, &w.pubkey(), &t)], &[&w])
        .expect("Claim");
    assert_err(
        first.send(vec![s.claim_ix(4, &w.pubkey(), &t)], &[&w]),
        E::AlreadyClaimed,
    );
    let ops = refund_all(&mut c, &s);
    assert_eq!(ops, escrow(&season) % 12);
    assert_eq!(ops, ops_after_abort(&season));
    // The world chunks and nations are closed for their rent (13 at most
    // per transaction).
    let targets = s.all_targets();
    let before = c.svm.get_balance(&admin.pubkey()).unwrap();
    let rent: u64 = targets
        .iter()
        .map(|t| c.svm.get_balance(&s.target(*t)).unwrap())
        .sum();
    for batch in targets.chunks(13) {
        c.send(
            vec![s.close_accounts_ix(&admin.pubkey(), batch.to_vec())],
            &[&admin],
        )
        .expect("CloseSeasonAccounts");
    }
    let fees = 2 * 5_000;
    assert_eq!(
        c.svm.get_balance(&admin.pubkey()).unwrap(),
        before + rent - fees
    );
    assert!(targets.iter().all(|t| c.owner(&s.target(*t)).is_none()));
    println!(
        "CloseSeasonAccounts: {} targets, {rent} lamports",
        targets.len()
    );
}

/// WP14 test 2: the operator may cancel while registering, free of charge
/// (the escrow comes back); anyone may from `start_by` + a day.
#[test]
fn registering_abort_rules() {
    let mut c = Chain::new();
    let s = twelve(&mut c);
    let outsider = c.funded();
    let start_by = s.season(&c).start_by;
    let mut late = c.fork();
    late.set_time(start_by + DAY - 1);
    assert_err(
        late.send(vec![s.abort_ix(&outsider.pubkey())], &[&outsider]),
        E::TooEarly,
    );
    late.set_time(start_by + DAY);
    late.send(vec![s.abort_ix(&outsider.pubkey())], &[&outsider])
        .expect("Abort by anyone after start_by + a day");
    let crank = s.crank.insecure_clone();
    c.send(vec![s.abort_ix(&crank.pubkey())], &[&crank])
        .expect("the operator cancels at once");
    let season = s.season(&c);
    assert_eq!(season.aborted_from, SeasonStatus::Registering as u8);
    assert_err(
        c.send(vec![s.abort_ix(&crank.pubkey())], &[&crank]),
        E::WrongStatus,
    );
    assert_eq!(refund_all(&mut c, &s), escrow(&season), "the whole escrow");
}

/// WP14 test 3: a Running world never delegated (chunk 0 on base, not
/// finished) is abortable a day after its tick deadline.
#[test]
fn running_never_delegated_aborts_a_day_after_the_tick_deadline() {
    let mut c = Chain::new();
    let s = twelve(&mut c);
    s.genesis(&mut c);
    s.seat_and_open(&mut c);
    let stage = stage_now(&mut c, &s);
    let at = stage.max(s.meta(&c).deadline) + DAY;
    let anyone = c.funded();
    c.set_time(at - 1);
    assert_err(
        c.send(vec![s.abort_ix(&anyone.pubkey())], &[&anyone]),
        E::TooEarly,
    );
    // Running needs the world's chunks.
    let mut ix = s.abort_ix(&anyone.pubkey());
    ix.accounts.truncate(10);
    c.set_time(at);
    assert_err(c.send(vec![ix], &[&anyone]), E::WorldTooSmall);
    let mut ix = s.abort_ix(&anyone.pubkey());
    ix.accounts[5].pubkey = s.nations[0];
    assert_err(c.send(vec![ix], &[&anyone]), E::WrongPda);
    c.send(vec![s.abort_ix(&anyone.pubkey())], &[&anyone])
        .expect("Abort");
    assert_eq!(s.season(&c).aborted_from, SeasonStatus::Running as u8);
    refund_all(&mut c, &s);
}

/// WP14 test 6(a): a delegated world is abortable only at the running
/// deadline.
#[test]
fn running_delegated_world_aborts_only_at_the_running_deadline() {
    let mut c = Chain::new();
    let s = twelve(&mut c);
    s.genesis(&mut c);
    s.seat_and_open(&mut c);
    let stage = stage_now(&mut c, &s);
    let deadline = s.meta(&c).deadline;
    let targets = s.all_targets();
    delegated(&mut c, &s, &targets);
    let rd = running_deadline(&s.season(&c), s.rules().ticks_per_season);
    let anyone = c.funded();
    // Rule (i) (never fully delegated) no longer applies.
    c.set_time(stage.max(deadline) + DAY);
    assert_err(
        c.send(vec![s.abort_ix(&anyone.pubkey())], &[&anyone]),
        E::TooEarly,
    );
    c.set_time(rd - 1);
    assert_err(
        c.send(vec![s.abort_ix(&anyone.pubkey())], &[&anyone]),
        E::TooEarly,
    );
    c.set_time(rd);
    c.send(vec![s.abort_ix(&anyone.pubkey())], &[&anyone])
        .expect("Abort at the running deadline");
    refund_all(&mut c, &s);
}

/// WP14 test 6(b): a finished, consistent world back on base must be
/// finished, not aborted (until a week past the deadline); a mixed or
/// rolled-back one is abortable at the deadline.
#[test]
fn a_finished_world_back_on_base_must_finish_not_abort() {
    let mut c = Chain::new();
    let s = twelve(&mut c);
    s.genesis(&mut c);
    s.seat_and_open(&mut c);
    stage_now(&mut c, &s);
    let old = c.data(&s.chunks[2]);
    s.play_tick(&mut c);
    delegated(&mut c, &s, &[]);
    s.fast_forward_to_end(&mut c);
    let rd = running_deadline(&s.season(&c), s.rules().ticks_per_season);
    let anyone = c.funded();
    c.set_time(rd);
    assert_err(
        c.send(vec![s.abort_ix(&anyone.pubkey())], &[&anyone]),
        E::TooEarly,
    );
    let mut week = c.fork();
    week.set_time(rd + FINISH_GRACE_SECONDS);
    week.send(vec![s.abort_ix(&anyone.pubkey())], &[&anyone])
        .expect("Abort a week past the running deadline");
    // One chunk from an older write: not finishable.
    let mut mixed = c.fork();
    mixed.set_data(&s.chunks[2], old);
    mixed
        .send(vec![s.abort_ix(&anyone.pubkey())], &[&anyone])
        .expect("Abort a mixed world at the running deadline");
    // Rolled back: not finishable.
    let mut rolled = c.fork();
    rolled.edit::<Season>(&s.season, |x| x.rolled_back = 1 << 9);
    rolled
        .send(vec![s.abort_ix(&anyone.pubkey())], &[&anyone])
        .expect("Abort a rolled-back world at the running deadline");
}

/// WP14 test 6(e): RequestUndelegation needs the operator, a delegated
/// target of the season, and a done season (or the running deadline).
#[test]
fn request_undelegation_gates() {
    let mut c = Chain::new().with_dlp_escape();
    let s = twelve(&mut c);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    assert_err(
        c.send(vec![s.request_undelegation_ix(&cp, 3)], &[&crank]),
        E::WrongStatus,
    );
    s.genesis(&mut c);
    s.seat_and_open(&mut c);
    s.fast_forward_to_end(&mut c);
    c.send(vec![s.finish_ix(false)], &[&crank])
        .expect_err("roster pending");
    c.advance(ROSTER_GRACE_SECONDS);
    c.send(vec![s.finish_ix(false)], &[&crank])
        .expect("FinishSeason");
    // The target's bit must be set (it was delegated this season).
    assert_err(
        c.send(vec![s.request_undelegation_ix(&cp, 3)], &[&crank]),
        E::InvalidParams,
    );
    delegated(&mut c, &s, &[3]);
    let outsider = c.funded();
    assert_err(
        c.send(
            vec![s.request_undelegation_ix(&outsider.pubkey(), 3)],
            &[&outsider],
        ),
        E::Unauthorized,
    );
    // Not a target of this season: a nation beyond its nations.
    let mut ix = s.request_undelegation_ix(&cp, NATION_TARGET + 1);
    ix.data = borsh::to_vec(&instruction::ChainInstruction::RequestUndelegation {
        target: NATION_TARGET + 5,
    })
    .unwrap();
    assert_err(c.send(vec![ix], &[&crank]), E::InvalidParams);
    // Another target's account.
    let mut ix = s.request_undelegation_ix(&cp, 3);
    ix.accounts[2].pubkey = s.chunks[4];
    assert_err(c.send(vec![ix], &[&crank]), E::WrongPda);
    let mut ix = s.request_undelegation_ix(&cp, 3);
    ix.accounts[8].pubkey = addr(SYSTEM);
    assert_err(c.send(vec![ix], &[&crank]), E::WrongDelegationProgram);
    let mut ix = s.request_undelegation_ix(&cp, 3);
    ix.accounts[3].pubkey = addr(SYSTEM);
    assert_program_err(c.send(vec![ix], &[&crank]), "IncorrectProgramId");
    mocks::take();
    c.send(vec![s.request_undelegation_ix(&cp, 3)], &[&crank])
        .expect("RequestUndelegation, Finalized");
    let calls = mocks::take();
    let call = calls.iter().find(|x| x.0 == "dlp").expect("the DLP call");
    assert_eq!(call.2, 26u64.to_le_bytes().to_vec());
    assert_eq!(call.1[..3], [cp, s.chunks[3], c.program]);
}

/// A10 / C13 (with WP02's skip rule): during Running the operator cannot
/// take a delegated account off the validator before the running deadline,
/// not even one whose undelegation step already ran (chunk 0's base copy
/// finished, its counter past the target: WP02 r2's early request). A
/// dropped undelegation intent waits for the deadline; the honest wind-up
/// instead relies on UndelegatePart skipping a target that already left
/// the ER (WP02, unit P4).
#[test]
fn request_undelegation_while_running_is_too_early_before_the_running_deadline() {
    let mut c = Chain::new().with_dlp_escape();
    let s = twelve(&mut c);
    s.genesis(&mut c);
    s.seat_and_open(&mut c);
    stage_now(&mut c, &s);
    s.fast_forward_to_end(&mut c);
    let chunks = s.chunks.clone();
    c.with_world(&chunks, |w| {
        let mut m = w.meta().unwrap();
        m.undelegated = 5;
        w.set_meta(&m).unwrap();
    });
    let targets = s.all_targets();
    delegated(&mut c, &s, &targets);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    let rd = running_deadline(&s.season(&c), s.rules().ticks_per_season);
    c.set_time(rd - 1);
    for t in [1, 7, 0] {
        assert_err(
            c.send(vec![s.request_undelegation_ix(&cp, t)], &[&crank]),
            E::TooEarly,
        );
    }
    c.set_time(rd);
    c.send(vec![s.request_undelegation_ix(&cp, 1)], &[&crank])
        .expect("RequestUndelegation at the running deadline");
}

/// WP14 §3.6 with the stand-in: after a request, anyone rolls the target
/// back; its last finalized bytes are restored, the season records it, and
/// FinishSeason refuses the world (refunds instead: Abort at the deadline).
#[test]
fn rollback_restores_the_saved_bytes_and_blocks_finish() {
    let mut c = Chain::new().with_dlp_escape();
    let s = twelve(&mut c);
    s.genesis(&mut c);
    s.seat_and_open(&mut c);
    stage_now(&mut c, &s);
    s.fast_forward_to_end(&mut c);
    delegated(&mut c, &s, &[7]);
    let saved = c.data(&s.chunks[7]);
    let crank = s.crank.insecure_clone();
    let anyone = c.funded();
    // Not delegated: nothing to roll back.
    assert_err(
        c.send(vec![s.rollback_ix(3, &crank.pubkey())], &[&anyone]),
        E::WrongDelegationProgram,
    );
    let mut ix = s.rollback_ix(7, &crank.pubkey());
    ix.accounts[2].pubkey = addr(SYSTEM);
    assert_program_err(c.send(vec![ix], &[&anyone]), "IncorrectProgramId");
    let rd = running_deadline(&s.season(&c), s.rules().ticks_per_season);
    c.set_time(rd);
    c.send(
        vec![s.request_undelegation_ix(&crank.pubkey(), 7)],
        &[&crank],
    )
    .expect("RequestUndelegation");
    let l = c
        .send(vec![s.rollback_ix(7, &crank.pubkey())], &[&anyone])
        .expect("RollbackUndelegation");
    assert_eq!(c.owner(&s.chunks[7]), Some(c.program));
    assert_eq!(c.data(&s.chunks[7]), saved, "the last finalized bytes");
    assert_eq!(s.season(&c).rolled_back, 1 << 7);
    let hash = solana_program::hash::hashv(&[&saved]).to_bytes();
    assert_eq!(
        record(&l.logs, b"PS_ROLLBACK")[1..],
        [
            s.p.id.to_le_bytes().to_vec(),
            7u16.to_le_bytes().to_vec(),
            hash.to_vec()
        ]
    );
    c.advance(ROSTER_GRACE_SECONDS);
    assert_err(
        c.send(vec![s.finish_ix(false)], &[&anyone]),
        E::WorldRolledBack,
    );
    c.send(vec![s.abort_ix(&anyone.pubkey())], &[&anyone])
        .expect("Abort (ii): not finishable at the deadline");
}

/// WP14 test 6(c): closing needs a done season, the operator, a
/// program-owned target of the season, each once.
#[test]
fn close_season_accounts_gates() {
    let mut c = Chain::new();
    let s = twelve(&mut c);
    s.genesis(&mut c);
    s.seat_and_open(&mut c);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    assert_err(
        c.send(vec![s.close_accounts_ix(&cp, vec![1])], &[&crank]),
        E::WrongStatus,
    );
    stage_now(&mut c, &s);
    c.advance(s.meta(&c).deadline - c.now + DAY);
    let anyone = c.funded();
    c.send(vec![s.abort_ix(&anyone.pubkey())], &[&anyone])
        .expect("Abort");
    assert_err(
        c.send(
            vec![s.close_accounts_ix(&anyone.pubkey(), vec![1])],
            &[&anyone],
        ),
        E::Unauthorized,
    );
    let mut ix = s.close_accounts_ix(&cp, vec![1, 2]);
    ix.accounts.pop();
    assert_err(c.send(vec![ix], &[&crank]), E::InvalidParams);
    assert_err(
        c.send(vec![s.close_accounts_ix(&cp, vec![1, 1])], &[&crank]),
        E::InvalidParams,
    );
    let mut ix = s.close_accounts_ix(&cp, vec![1]);
    ix.accounts[2].pubkey = s.chunks[2];
    assert_err(c.send(vec![ix], &[&crank]), E::WrongPda);
    c.set_owner(&s.chunks[5], addr(DLP));
    assert_err(
        c.send(vec![s.close_accounts_ix(&cp, vec![5])], &[&crank]),
        E::WrongWorld,
    );
    c.send(
        vec![s.close_accounts_ix(&cp, vec![1, NATION_TARGET + 1])],
        &[&crank],
    )
    .expect("CloseSeasonAccounts");
    assert!(c.owner(&s.chunks[1]).is_none() && c.owner(&s.nations[1]).is_none());
}

/// WP14: a closed account cannot be re-created (AllocWorld refuses after
/// Registering).
#[test]
#[ignore = "until WP14 (unit P2): AllocWorld and AllocNation refuse after Registering"]
fn a_closed_account_is_not_re_allocated() {
    let mut c = Chain::new();
    let s = twelve(&mut c);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    c.send(vec![s.abort_ix(&cp)], &[&crank]).expect("cancel");
    c.send(vec![s.close_accounts_ix(&cp, vec![1])], &[&crank])
        .expect("CloseSeasonAccounts");
    assert_err(
        c.send(vec![s.alloc_world_ix(&cp, 1)], &[&crank]),
        E::WrongStatus,
    );
}

/// WP14 test 6(f), against a real delegation program (`DLP_SO`, dlp-api
/// ≥ 3.1.0): the rollback restores the bytes of the last finalized commit.
#[test]
#[ignore = "needs DLP_SO (a delegation program >= 3.1.0 build)"]
fn rollback_restores_last_finalized_bytes() {
    let Some(mut c) = Chain::new().with_magicblock().with_real_dlp() else {
        return;
    };
    let s = twelve(&mut c);
    s.genesis(&mut c);
    s.seat_and_open(&mut c);
    stage_now(&mut c, &s);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    c.send(vec![s.delegate_ix(&cp, 7)], &[&crank])
        .expect("Delegate chunk 7");
    c.edit::<Season>(&s.season, |x| x.delegated |= 1 << 7);
    let pattern: Vec<u8> = (0..CHUNK).map(|i| (i % 251) as u8).collect();
    c.set_data(&s.chunks[7], pattern.clone());
    let rd = running_deadline(&s.season(&c), s.rules().ticks_per_season);
    c.set_time(rd);
    c.send(vec![s.request_undelegation_ix(&cp, 7)], &[&crank])
        .expect("RequestUndelegation");
    let slot = c.svm.get_sysvar::<solana_clock::Clock>().slot;
    c.svm.warp_to_slot(slot + 9_000);
    c.advance(1);
    let anyone = c.funded();
    c.send(vec![s.rollback_ix(7, &cp)], &[&anyone])
        .expect("RollbackUndelegation");
    assert_eq!(c.owner(&s.chunks[7]), Some(c.program));
    assert_eq!(c.data(&s.chunks[7]), pattern);
    assert_eq!(s.season(&c).rolled_back, 1 << 7);
    s.fast_forward_to_end(&mut c);
    assert_err(
        c.send(vec![s.finish_ix(false)], &[&anyone]),
        E::WorldRolledBack,
    );
    c.send(vec![s.abort_ix(&anyone.pubkey())], &[&anyone])
        .expect("Abort (ii)");
}

/// WP14 test 6(g), against a real delegation program (`DLP_SO`): after an
/// owner request, the validator's plain `Undelegate` (without the request
/// PDA) fails, which is why requests are refused during play and wind-up.
#[test]
#[ignore = "needs DLP_SO (a delegation program >= 3.1.0 build)"]
fn an_owner_request_blocks_the_plain_undelegate() {
    let Some(mut c) = Chain::new().with_magicblock().with_real_dlp() else {
        return;
    };
    let s = twelve(&mut c);
    s.genesis(&mut c);
    s.seat_and_open(&mut c);
    stage_now(&mut c, &s);
    let crank = s.crank.insecure_clone();
    let cp = crank.pubkey();
    c.send(vec![s.delegate_ix(&cp, 7)], &[&crank])
        .expect("Delegate chunk 7");
    c.edit::<Season>(&s.season, |x| x.delegated |= 1 << 7);
    let rd = running_deadline(&s.season(&c), s.rules().ticks_per_season);
    c.set_time(rd);
    c.send(vec![s.request_undelegation_ix(&cp, 7)], &[&crank])
        .expect("RequestUndelegation");
    // dlp-api 3.1.0 `undelegate` (discriminator 3), without the request.
    let key = s.chunks[7];
    let dlp = addr(DLP);
    let p = |seeds: &[&[u8]]| pda(&dlp, seeds);
    let validator = c.funded();
    let vk = validator.pubkey();
    let ix = solana_instruction::Instruction {
        program_id: dlp,
        accounts: vec![
            ws(&vk),
            w(&key),
            r(&c.program),
            w(&p(&[b"undelegate-buffer", key.as_ref()])),
            r(&p(&[b"state-diff", key.as_ref()])),
            r(&p(&[b"commit-state-record", key.as_ref()])),
            w(&p(&[b"delegation", key.as_ref()])),
            w(&p(&[b"delegation-metadata", key.as_ref()])),
            w(&cp),
            w(&p(&[b"fees-vault"])),
            w(&p(&[b"v-fees-vault", vk.as_ref()])),
            r(&addr(SYSTEM)),
        ],
        data: 3u64.to_le_bytes().to_vec(),
    };
    let err = c.send(vec![ix], &[&validator]).expect_err("Undelegate");
    // `DlpError::MissingUndelegationRequest`.
    assert_eq!(err.code, Some(54), "{err:#?}");
}

/// Abort of a Running season whose world is home and finished (the
/// costliest path: 20 PDA checks and FinishSeason's precheck, which hashes
/// the body) at the largest shape the suite plays: six nations, 48
/// members. Printed for the client's profile choice.
#[test]
fn abort_cost_at_the_member_cap() {
    let mut c = Chain::new();
    let s = SeasonFx::running(
        &mut c,
        Params {
            nations: 6,
            ..Params::default()
        },
        SEASON_MEMBER_CAP as usize,
    );
    stage_now(&mut c, &s);
    s.fast_forward_to_end(&mut c);
    let chunks = s.chunks.clone();
    let body = c.with_world(&chunks, |w| w.body(&WORLD_MAGIC).unwrap().len());
    let rd = running_deadline(&s.season(&c), s.rules().ticks_per_season);
    c.set_time(rd + FINISH_GRACE_SECONDS);
    let anyone = c.funded();
    let need = c
        .need(&[s.abort_ix(&anyone.pubkey())], &[&anyone])
        .expect("Abort lands");
    println!("Abort, world body {body} B: {need:?}");
}
