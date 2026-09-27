//! FinishSeason, Claim, WithdrawOps: payouts from the final world, and every
//! permission, mint, vault and status check around the money (sigverify on).

use permutation_chain_svm_tests::error::ChainError as E;
use permutation_chain_svm_tests::finalize::{finalize, Roster};
use permutation_chain_svm_tests::payout::claim_amount;
use permutation_chain_svm_tests::state::*;
use permutation_chain_svm_tests::*;
use permutation_rules::history::{history_root, season_record};
use solana_signer::Signer;

const DEP: [u64; 4] = [0, 5_000_000, 0, 3_000_000];

/// Four members (two per nation, with deposits), seated and open.
fn running(c: &mut Chain, id: u64) -> SeasonFx {
    let mut s = SeasonFx::create(
        c,
        Params {
            id,
            ..Params::default()
        },
    );
    for (i, d) in DEP.iter().enumerate() {
        s.register(c, (i % 2) as u16, *d);
    }
    s.genesis(c);
    s.seat_and_open(c);
    s
}

/// A finalized season: two ticks played, then fast-forwarded to the end.
fn finalized(c: &mut Chain, id: u64) -> SeasonFx {
    let s = running(c, id);
    s.play_tick(c);
    s.play_tick(c);
    s.fast_forward_to_end(c);
    let crank = s.crank.insecure_clone();
    c.send(vec![s.finish_ix(false)], &[&crank])
        .expect("FinishSeason");
    s
}

#[test]
fn finish_season_checks() {
    let mut c = Chain::new();
    let anyone = c.funded();
    // Not Running (still registering): no world yet, so the precheck
    // (shared with Abort) refuses it first.
    let b = SeasonFx::create(
        &mut c,
        Params {
            id: 8,
            ..Params::default()
        },
    );
    assert_err(c.send(vec![b.finish_ix(false)], &[&anyone]), E::WrongWorld);
    let s = running(&mut c, 7);
    s.play_tick(&mut c);
    assert_err(
        c.send(vec![s.finish_ix(false)], &[&anyone]),
        E::SeasonNotOver,
    );
    s.fast_forward_to_end(&mut c);
    // A chunk still delegated (owned by the delegation program).
    c.set_owner(&s.chunks[4], addr(DLP));
    assert_err(c.send(vec![s.finish_ix(false)], &[&anyone]), E::WrongWorld);
    c.set_owner(&s.chunks[4], c.program);
    // Chunk 0 still holds a genesis job.
    let world0 = c.data(&s.chunks[0]);
    let mut genesis = world0.clone();
    genesis[..8].copy_from_slice(&GENESIS_MAGIC);
    c.set_data(&s.chunks[0], genesis);
    assert_err(c.send(vec![s.finish_ix(false)], &[&anyone]), E::WrongWorld);
    c.set_data(&s.chunks[0], world0);
    // A chunk taken back by RollbackUndelegation (WP14).
    c.edit::<Season>(&s.season, |x| x.rolled_back = 1 << 5);
    assert_err(
        c.send(vec![s.finish_ix(false)], &[&anyone]),
        E::WorldRolledBack,
    );
    c.edit::<Season>(&s.season, |x| x.rolled_back = 0);
    // The vault: missing, not the season's PDA, or the roster in its slot.
    let mut ix = s.finish_ix(false);
    ix.accounts.pop();
    assert_program_err(c.send(vec![ix], &[&anyone]), "NotEnoughAccountKeys");
    let fake_vault = c.token_account(&s.mint, &s.season, 1_000_000_000);
    let mut ix = s.finish_ix(false);
    ix.accounts[21].pubkey = fake_vault;
    assert_err(c.send(vec![ix], &[&anyone]), E::WrongPda);
    let mut ix = s.finish_ix(false);
    ix.accounts[21].pubkey = s.roster;
    assert_err(c.send(vec![ix], &[&anyone]), E::WrongPda);
    // Permissionless: the payouts are the finalize of the final world.
    let before = s.season(&c);
    let state = s.world(&mut c);
    let rules = s.rules();
    let f = finalize(&before, &state, &rules, Roster::None, false);
    assert!(!f.voided);
    let l = c
        .send(vec![s.finish_ix(false)], &[&anyone])
        .expect("FinishSeason");
    let season = s.season(&c);
    assert_eq!(season.status, SeasonStatus::Finalized);
    assert_eq!(season.payouts, f.payouts);
    assert_eq!(season.payouts.len(), 4);
    assert_eq!((season.pool, season.ops), (f.pool, f.ops));
    assert_eq!(season.treasury_final, f.treasury_final);
    assert_eq!(season.refund_base, f.refund_base);
    assert_eq!((season.voided, season.outstanding), (false, f.owed));
    assert_eq!(season.outstanding, c.balance(&s.vault), "every unit owed");
    assert_eq!(season.final_root, state.state_root().unwrap());
    let record = season_record(&state, &f.settlement);
    assert_eq!(
        season.history_root,
        history_root(&before.prev_history_root, &record)
    );
    let h = record_of(&l.logs);
    assert_eq!(
        h[1..4],
        [
            7u64.to_le_bytes().to_vec(),
            before.prev_history_root.to_vec(),
            season.history_root.to_vec()
        ]
    );
    assert_eq!(
        h[4],
        borsh::to_vec(&record).unwrap(),
        "the season record, in full"
    );
    assert!(
        h[4].len() <= HISTORY_LOG_MAX && !log_truncated(&l.logs) && log_bytes(&l.logs) <= 10_000
    );
    assert_err(c.send(vec![s.finish_ix(false)], &[&anyone]), E::WrongStatus);
}

/// WP01: a world whose chunks come from different writes (one chunk from
/// an older snapshot) is refused, not decoded and paid out.
#[test]
fn a_mixed_world_is_refused() {
    let mut c = Chain::new();
    let s = running(&mut c, 7);
    let anyone = c.funded();
    let old: Vec<Vec<u8>> = s.chunks.iter().map(|k| c.data(k)).collect();
    s.play_tick(&mut c);
    s.fast_forward_to_end(&mut c);
    // A chunk (not chunk 0, which holds the header) that the later writes
    // changed, put back as it was.
    let k = (1..WORLD_CHUNKS)
        .find(|k| c.data(&s.chunks[*k]) != old[*k])
        .expect("a later write changed a chunk");
    c.set_data(&s.chunks[k], old[k].clone());
    assert_err(c.send(vec![s.finish_ix(false)], &[&anyone]), E::WrongWorld);
}

/// WP15 test 13: a season whose recorded rules or settlement logic are
/// not this build's (or not the world's) is refused.
#[test]
fn finish_season_refuses_a_world_under_other_rules() {
    let mut c = Chain::new();
    let s = running(&mut c, 7);
    let anyone = c.funded();
    s.fast_forward_to_end(&mut c);
    let pinned = s.season(&c);
    c.edit::<Season>(&s.season, |x| x.rules_hash[0] ^= 1);
    assert_err(
        c.send(vec![s.finish_ix(false)], &[&anyone]),
        E::RulesMismatch,
    );
    c.edit::<Season>(&s.season, |x| {
        x.rules_hash = pinned.rules_hash;
        x.logic_version += 1;
    });
    assert_err(
        c.send(vec![s.finish_ix(false)], &[&anyone]),
        E::RulesMismatch,
    );
    c.edit::<Season>(&s.season, |x| x.logic_version = pinned.logic_version);
    c.send(vec![s.finish_ix(false)], &[&anyone])
        .expect("FinishSeason under the season's own rules");
}

/// WP12 §6.3(4): books that owe more than the vault holds are refused.
#[test]
fn finish_season_refuses_an_underfunded_vault() {
    let mut c = Chain::new();
    let s = running(&mut c, 7);
    let anyone = c.funded();
    s.fast_forward_to_end(&mut c);
    let full = c.balance(&s.vault);
    c.set_balance(&s.vault, full - 1);
    assert_err(c.send(vec![s.finish_ix(false)], &[&anyone]), E::Insolvent);
    assert_eq!(s.season(&c).status, SeasonStatus::Running);
    c.set_balance(&s.vault, full);
    c.send(vec![s.finish_ix(false)], &[&anyone])
        .expect("FinishSeason");
}

fn record_of(logs: &[String]) -> Vec<Vec<u8>> {
    record(logs, b"PS_HISTORY")
}

#[test]
fn claim_checks() {
    let mut c = Chain::new();
    let s = running(&mut c, 7);
    s.fast_forward_to_end(&mut c);
    let i = 1; // deposited 5 USDC into nation 1
    let w0 = s.members[i].wallet.insecure_clone();
    let t0 = s.members[i].token;
    let claim = |c: &mut Chain, ix| c.send(vec![ix], &[&w0]);
    // Before FinishSeason.
    assert_err(
        claim(&mut c, s.claim_ix(i, &w0.pubkey(), &t0)),
        E::WrongStatus,
    );
    let crank = s.crank.insecure_clone();
    c.send(vec![s.finish_ix(false)], &[&crank]).unwrap();
    let vault = c.balance(&s.vault);
    let thief = c.funded();
    let thief_token = c.token_account(&s.mint, &thief.pubkey(), 0);
    // Another wallet claims member i, to its own account.
    assert_err(
        c.send(
            vec![s.claim_ix(i, &thief.pubkey(), &thief_token)],
            &[&thief],
        ),
        E::Unauthorized,
    );
    // The wallet must sign (the thief pays).
    let mut ix = s.claim_ix(i, &w0.pubkey(), &t0);
    ix.accounts[0].is_signer = false;
    assert_err(c.send(vec![ix], &[&thief]), E::MissingSignature);
    // A destination the wallet does not own, or of another mint.
    assert_err(
        claim(&mut c, s.claim_ix(i, &w0.pubkey(), &thief_token)),
        E::WrongTokenAccount,
    );
    let other_mint = c.mint(6);
    let wrong_mint_dest = c.token_account(&other_mint, &w0.pubkey(), 0);
    assert_err(
        claim(&mut c, s.claim_ix(i, &w0.pubkey(), &wrong_mint_dest)),
        E::WrongTokenAccount,
    );
    // Another mint account; another token program.
    let mut ix = s.claim_ix(i, &w0.pubkey(), &t0);
    ix.accounts[5].pubkey = other_mint;
    assert_err(claim(&mut c, ix), E::WrongMint);
    let mut ix = s.claim_ix(i, &w0.pubkey(), &t0);
    ix.accounts[6].pubkey = addr(SYSTEM);
    assert_err(claim(&mut c, ix), E::WrongMint);
    // A vault that is not the season's PDA.
    let fake_vault = c.token_account(&s.mint, &s.season, 1_000_000_000);
    let mut ix = s.claim_ix(i, &w0.pubkey(), &t0);
    ix.accounts[3].pubkey = fake_vault;
    assert_err(claim(&mut c, ix), E::WrongPda);
    // A member of another finalized season, with this season and its vault.
    let b = finalized(&mut c, 8);
    let bw = b.members[0].wallet.insecure_clone();
    let mut ix = s.claim_ix(i, &bw.pubkey(), &b.members[0].token);
    ix.accounts[2].pubkey = b.members[0].member;
    assert_err(c.send(vec![ix], &[&bw]), E::NotInitialized);
    // None of it moved USDC or marked the claim.
    assert_eq!(c.balance(&s.vault), vault);
    assert!(!s.member(&c, i).claimed);
    // Never more than FinishSeason set aside (WP12).
    let owed = claim_amount(&s.season(&c), &s.member(&c, i));
    assert!(owed > 0);
    let outstanding = s.season(&c).outstanding;
    c.edit::<Season>(&s.season, |x| x.outstanding = owed - 1);
    assert_err(
        claim(&mut c, s.claim_ix(i, &w0.pubkey(), &t0)),
        E::Insolvent,
    );
    c.edit::<Season>(&s.season, |x| x.outstanding = outstanding);
    // The claim pays claim_amount to the wallet's own account, once.
    claim(&mut c, s.claim_ix(i, &w0.pubkey(), &t0)).expect("Claim");
    assert_eq!((c.balance(&t0), c.balance(&s.vault)), (owed, vault - owed));
    assert!(s.member(&c, i).claimed);
    assert_eq!(s.season(&c).outstanding, outstanding - owed);
    assert_err(
        claim(&mut c, s.claim_ix(i, &w0.pubkey(), &t0)),
        E::AlreadyClaimed,
    );
    // Nothing owed.
    c.edit::<Season>(&s.season, |x| x.payouts[0] = 0);
    let w = s.members[0].wallet.insecure_clone();
    assert_eq!(claim_amount(&s.season(&c), &s.member(&c, 0)), 0);
    assert_err(
        c.send(vec![s.claim_ix(0, &w.pubkey(), &s.members[0].token)], &[&w]),
        E::NothingToClaim,
    );
}

#[test]
fn withdraw_ops_checks() {
    let mut c = Chain::new();
    let s = running(&mut c, 7);
    s.fast_forward_to_end(&mut c);
    let admin = s.admin.insecure_clone();
    let ap = admin.pubkey();
    let dest = c.token_account(&s.mint, &ap, 0);
    let withdraw = |c: &mut Chain, ix| c.send(vec![ix], &[&admin]);
    // Before FinishSeason.
    assert_err(withdraw(&mut c, s.withdraw_ix(&ap, &dest)), E::WrongStatus);
    let crank = s.crank.insecure_clone();
    c.send(vec![s.finish_ix(false)], &[&crank]).unwrap();
    // The admin only (not the crank), and it must sign.
    let crank_dest = c.token_account(&s.mint, &crank.pubkey(), 0);
    assert_err(
        c.send(vec![s.withdraw_ix(&crank.pubkey(), &crank_dest)], &[&crank]),
        E::Unauthorized,
    );
    let mut ix = s.withdraw_ix(&ap, &dest);
    ix.accounts[0].is_signer = false;
    assert_err(c.send(vec![ix], &[&crank]), E::MissingSignature);
    // A destination of another mint; another mint account; another token program.
    let other_mint = c.mint(6);
    let wrong_dest = c.token_account(&other_mint, &ap, 0);
    assert_err(
        withdraw(&mut c, s.withdraw_ix(&ap, &wrong_dest)),
        E::WrongTokenAccount,
    );
    let mut ix = s.withdraw_ix(&ap, &dest);
    ix.accounts[4].pubkey = other_mint;
    assert_err(withdraw(&mut c, ix), E::WrongMint);
    let mut ix = s.withdraw_ix(&ap, &dest);
    ix.accounts[5].pubkey = addr(SYSTEM);
    assert_err(withdraw(&mut c, ix), E::WrongMint);
    // Another season's vault.
    let mut ix = s.withdraw_ix(&ap, &dest);
    ix.accounts[2].pubkey = pda(&c.program, &[VAULT_SEED, &8u64.to_le_bytes()]);
    assert_err(withdraw(&mut c, ix), E::WrongPda);
    // Never more than FinishSeason set aside (WP12).
    let (ops, outstanding) = (s.season(&c).ops, s.season(&c).outstanding);
    c.edit::<Season>(&s.season, |x| x.outstanding = ops - 1);
    assert_err(withdraw(&mut c, s.withdraw_ix(&ap, &dest)), E::Insolvent);
    c.edit::<Season>(&s.season, |x| x.outstanding = outstanding);
    // The operations share, once.
    let vault = c.balance(&s.vault);
    withdraw(&mut c, s.withdraw_ix(&ap, &dest)).expect("WithdrawOps");
    assert_eq!((c.balance(&dest), c.balance(&s.vault)), (ops, vault - ops));
    assert!(s.season(&c).ops_withdrawn);
    assert_eq!(s.season(&c).outstanding, outstanding - ops);
    assert_err(withdraw(&mut c, s.withdraw_ix(&ap, &dest)), E::WrongStatus);
}
