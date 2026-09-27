//! Base layer, after the season: payouts, claims and the operations share.

use permutation_rules::roster::roster_commit;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};

use super::accounts::*;
use crate::error::ChainError;
use crate::finalize::{finalize, Roster};
use crate::payout::claim_amount;
use crate::state::*;

/// What `FinishSeason` needs of the world, shared with `Abort` (whose rule
/// (ii) opens exactly when this fails): every chunk program-owned on base
/// and the season's PDA, chunk 0 a world (`WORLD_MAGIC`) whose meta says
/// the season is over, no chunk rolled back, every chunk from one write
/// (the root trailer), and the rules the season was created under. Returns
/// the world and its root (the stored body's hash).
pub(super) fn finish_precheck<'a, 'info>(
    program_id: &Pubkey,
    season: &Season,
    accounts: &'a [AccountInfo<'info>],
) -> Result<(Chunks<'a, 'info>, [u8; 32]), ProgramError> {
    let world = world_chunks(program_id, accounts, season.season_id)?;
    let root = precheck_world(season, &world)?;
    Ok((world, root))
}

/// `finish_precheck` on chunks whose owner and PDA were already checked.
pub(super) fn precheck_world(season: &Season, world: &Chunks) -> Result<[u8; 32], ProgramError> {
    if !world.is_world() {
        return Err(ChainError::WrongWorld.into());
    }
    if !world.meta()?.finished {
        return Err(ChainError::SeasonNotOver.into());
    }
    if season.rolled_back & CHUNK_BITS != 0 {
        return Err(ChainError::WorldRolledBack.into());
    }
    let root = world.root()?;
    world.check_root(&root)?;
    if world.ruleset_hash()? != season.rules_hash {
        return Err(ChainError::RulesMismatch.into());
    }
    require_rules(season)?;
    Ok(root)
}

/// `FinishSeason`: accounts 0 season (w) · 1..=20 world chunks · 21 vault ·
/// 22 roster (only with a revealed roster). Permissionless.
pub(super) fn finish_season(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let season_info = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let mut season = load_season(program_id, season_info)?;
    // Undelegated and back with its final state, from one write.
    let (world, final_root) = finish_precheck(program_id, &season, &accounts[1..])?;
    if season.status != SeasonStatus::Running {
        return Err(ChainError::WrongStatus.into());
    }
    let meta = world.meta()?;
    // Operator AI members (V5 §18.2): the roster must be revealed in full
    // and match the blinded commitment, unless the grace period after the
    // last tick has passed.
    let roster_account;
    let roster = if season.ai_count == 0 {
        Roster::None
    } else if season.roster_revealed == season.ai_count
        && roster_commit(&season.roster_blind, &season.roster_acc) == season.roster_commit
    {
        let roster_info = accounts
            .get(2 + WORLD_CHUNKS)
            .ok_or(ProgramError::NotEnoughAccountKeys)?;
        roster_account = load_roster(program_id, roster_info, season.season_id)?;
        Roster::Revealed(&roster_account.entries)
    } else {
        if now()? < meta.deadline.saturating_add(ROSTER_GRACE_SECONDS) {
            return Err(ChainError::RosterPending.into());
        }
        Roster::Forfeited
    };
    let vault_info = accounts
        .get(1 + WORLD_CHUNKS)
        .ok_or(ProgramError::NotEnoughAccountKeys)?;
    let vault = vault_amount(program_id, &season, season_info.key, vault_info)?;
    let rules = rules_for(season.preset, season.market)?;
    let state = world.read_world()?;
    if state.tick < rules.ticks_per_season {
        return Err(ChainError::SeasonNotOver.into());
    }
    // In-play income (market fees and tariffs) joins the pool and
    // operations as the fees do (V5 D11); a world that did not conserve
    // USDC, or books that do not balance, void the season (WP12).
    let f = finalize(&season, &state, &rules, roster, meta.usdc_broken);
    if f.owed > vault {
        return Err(ChainError::Insolvent.into());
    }
    // The history layer: this season's record, chained onto the one it
    // follows, and logged in full for the next season and the verifier.
    let record = permutation_rules::history::season_record_at(&state, &f.settlement, final_root);
    drop(state);
    season.history_root =
        permutation_rules::history::history_root(&season.prev_history_root, &record);
    let mut bytes = borsh::to_vec(&record).map_err(|_| ChainError::Rules)?;
    drop(record);
    // A transaction's logs are capped at 10 KB (base64): a record too large
    // to log is left out (the root is still stored, and the verifier
    // recomputes the record from the final world).
    if bytes.len() > HISTORY_LOG_MAX {
        bytes.clear();
    }
    solana_program::log::sol_log_data(&[
        b"PS_HISTORY",
        &season.season_id.to_le_bytes(),
        &season.prev_history_root,
        &season.history_root,
        &bytes,
    ]);
    season.pool = f.pool;
    season.ops = f.ops;
    season.payouts = f.payouts;
    season.treasury_final = f.treasury_final;
    season.refund_base = f.refund_base;
    season.refund_in_payout = f.refund_in_payout;
    season.roster_outcome = roster.outcome();
    season.bounty_paid = f.bounty_paid;
    season.voided = f.voided;
    season.outstanding = f.owed;
    season.final_root = final_root;
    season.status = SeasonStatus::Finalized;
    store(&mut season_info.try_borrow_mut_data()?, &season)?;
    msg!(
        "PS season {} finalized: pool {} ops {} owed {} vault {}{}",
        season.season_id,
        season.pool,
        season.ops,
        season.outstanding,
        vault,
        if season.voided {
            " VOIDED: USDC not conserved; fees, deposits and escrow returned"
        } else {
            ""
        }
    );
    Ok(())
}

/// `Claim`: a member's payout and refund (`claim_amount`), once, to the
/// wallet's own token account; after `FinishSeason` or `Abort`. A season of
/// the previous layout (`PSSEASN7`) still pays by its own rule and keeps no
/// `outstanding`.
pub(super) fn claim(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let it = &mut accounts.iter();
    let wallet = next_account_info(it)?;
    let season_info = next_account_info(it)?;
    let member_info = next_account_info(it)?;
    let vault = next_account_info(it)?;
    let dest = next_account_info(it)?;
    let mint = next_account_info(it)?;
    let token_program = next_account_info(it)?;
    signer(wallet)?;
    let (mut season, legacy) = load_season_compat(program_id, season_info)?;
    if !matches!(
        season.status,
        SeasonStatus::Finalized | SeasonStatus::Aborted
    ) {
        return Err(ChainError::WrongStatus.into());
    }
    let mut member = load_member(program_id, member_info, season.season_id)?;
    if wallet.key.as_ref() != member.wallet {
        return Err(ChainError::Unauthorized.into());
    }
    if member.claimed {
        return Err(ChainError::AlreadyClaimed.into());
    }
    let amount = claim_amount(&season, &member);
    if amount == 0 {
        return Err(ChainError::NothingToClaim.into());
    }
    check_usdc(program_id, &season, mint, token_program, vault)?;
    check_token_account(dest, mint, Some(wallet.key))?;
    // Never more than FinishSeason (or Abort) set aside (WP12).
    if !legacy {
        season.outstanding = season
            .outstanding
            .checked_sub(amount)
            .ok_or(ChainError::Insolvent)?;
    }
    member.claimed = true;
    store(&mut member_info.try_borrow_mut_data()?, &member)?;
    if !legacy {
        store(&mut season_info.try_borrow_mut_data()?, &season)?;
    }
    pay_from_vault(&season, season_info, vault, mint, dest, amount)?;
    msg!(
        "PS claim season {} member {} amount {}",
        season.season_id,
        member.index,
        amount
    );
    Ok(())
}

/// `WithdrawOps`: the admin's operations share, once: `ops` after
/// `FinishSeason`, `lifecycle::ops_after_abort` after `Abort`.
pub(super) fn withdraw_ops(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let it = &mut accounts.iter();
    let admin = next_account_info(it)?;
    let season_info = next_account_info(it)?;
    let vault = next_account_info(it)?;
    let dest = next_account_info(it)?;
    let mint = next_account_info(it)?;
    let token_program = next_account_info(it)?;
    signer(admin)?;
    let (mut season, legacy) = load_season_compat(program_id, season_info)?;
    if admin.key.as_ref() != season.admin {
        return Err(ChainError::Unauthorized.into());
    }
    let amount = match season.status {
        SeasonStatus::Finalized => season.ops,
        SeasonStatus::Aborted => crate::lifecycle::ops_after_abort(&season),
        _ => return Err(ChainError::WrongStatus.into()),
    };
    if season.ops_withdrawn {
        return Err(ChainError::WrongStatus.into());
    }
    check_usdc(program_id, &season, mint, token_program, vault)?;
    check_token_account(dest, mint, None)?;
    if !legacy {
        season.outstanding = season
            .outstanding
            .checked_sub(amount)
            .ok_or(ChainError::Insolvent)?;
    }
    season.ops_withdrawn = true;
    store(&mut season_info.try_borrow_mut_data()?, &season)?;
    if amount > 0 {
        pay_from_vault(&season, season_info, vault, mint, dest, amount)?;
    }
    msg!("PS operations share {} withdrawn", amount);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(index: u32, civ: u16, shares: u64) -> MemberAccount {
        MemberAccount {
            magic: MEMBER_MAGIC,
            season_id: 1,
            bump: 0,
            index,
            civ,
            wallet: [0; 32],
            session: [0; 32],
            kind: 0,
            name: String::new(),
            attestation: [0; 32],
            stand: 0,
            votes: [0; 4],
            shares,
            claimed: false,
            tag: [0; 32],
        }
    }

    /// A finalized two-nation season of three members.
    fn finalized_season(treasury: Vec<u64>, treasury_final: Vec<u64>, payouts: Vec<u64>) -> Season {
        Season {
            magic: SEASON_MAGIC,
            season_id: 1,
            bump: 0,
            vault_bump: 0,
            admin: [0; 32],
            crank: [0; 32],
            usdc_mint: [0; 32],
            usdc_decimals: 6,
            preset: 0,
            nations: 2,
            entry_fee: 0,
            tick_seconds: 1,
            market: true,
            status: SeasonStatus::Finalized,
            world_seed: [0; 32],
            season_seed: [0; 32],
            member_count: 3,
            nation_members: vec![2, 1],
            seated: 3,
            pool: 0,
            ops: 0,
            ops_withdrawn: false,
            treasury,
            treasury_final,
            payouts,
            final_root: [0; 32],
            prev_season_id: 0,
            prev_history_root: [0; 32],
            history_root: [0; 32],
            ai_count: 0,
            roster_commit: [0; 32],
            bounty_each: 0,
            bond: 0,
            roster_acc: [0; 32],
            roster_revealed: 0,
            roster_outcome: 0,
            bounty_paid: Vec::new(),
            delegated: 0,
            roster_blind: [0; 32],
            refund_base: Vec::new(),
            refund_in_payout: Vec::new(),
            seed_state: 0,
            seed_oracle: [0; 32],
            seed_requested_at: 0,
            seed_requests: 0,
            deposit: 0,
            outstanding: 0,
            voided: false,
            start_by: 0,
            stage_at: 0,
            rolled_back: 0,
            aborted_from: 0,
            validator: [0; 32],
            rules_version: 0,
            rules_hash: [0; 32],
            logic_version: 0,
            created_slot: 0,
        }
    }

    /// Prizes plus pro-rata treasury refunds never pay out more than the
    /// treasury has left.
    #[test]
    fn claims_split_the_treasury_by_shares() {
        let season = finalized_season(vec![300, 0], vec![100, 0], vec![5, 6, 7]);
        let (a, b, c) = (member(0, 0, 200), member(1, 0, 100), member(2, 1, 0));
        assert_eq!(claim_amount(&season, &a), 5 + 66);
        assert_eq!(claim_amount(&season, &b), 6 + 33);
        assert_eq!(claim_amount(&season, &c), 7);
        let out_of_range = member(9, 5, 10);
        assert_eq!(claim_amount(&season, &out_of_range), 0);
    }

    /// WP10 C4: a flagged member (a revealed AI) gets its payout alone; the
    /// others share `treasury_final` over `refund_base`; an empty
    /// `refund_base` is the legacy rule (over `treasury`).
    #[test]
    fn claims_follow_the_refund_base_and_the_flags() {
        let mut season = finalized_season(vec![300, 0], vec![100, 0], vec![5, 6, 7]);
        season.refund_base = vec![200, 0];
        season.refund_in_payout = vec![0b001];
        let (a, b, c) = (member(0, 0, 100), member(1, 0, 200), member(2, 1, 0));
        assert_eq!(claim_amount(&season, &a), 5, "flagged: the payout alone");
        assert_eq!(claim_amount(&season, &b), 6 + 100, "200 of 200 shares");
        assert_eq!(claim_amount(&season, &c), 7, "base 0: the prize only");
        season.refund_base.clear();
        season.refund_in_payout.clear();
        assert_eq!(
            claim_amount(&season, &b),
            6 + 66,
            "legacy: over the deposits"
        );
    }

    /// WP14: after an abort a claim is the fee and the deposit, plus an
    /// equal share of a forfeited escrow; payouts and final treasuries are
    /// ignored.
    #[test]
    fn claims_after_abort_refund_fee_deposit_and_forfeit() {
        let mut season = finalized_season(vec![300, 0], vec![100, 0], vec![5, 6, 7]);
        season.status = SeasonStatus::Aborted;
        season.entry_fee = 1_000;
        (season.ai_count, season.bounty_each, season.bond) = (1, 10, 20);
        season.aborted_from = SeasonStatus::Registering as u8;
        assert_eq!(claim_amount(&season, &member(0, 0, 200)), 1_000 + 200);
        season.aborted_from = SeasonStatus::Running as u8;
        assert_eq!(
            claim_amount(&season, &member(0, 0, 200)),
            1_000 + 200 + 30 / 3
        );
        assert_eq!(claim_amount(&season, &member(2, 1, 0)), 1_000 + 10);
    }

    /// WP12 test 8: the claims and the operations share use up exactly what
    /// FinishSeason set aside (`outstanding`), rounding included, in a
    /// normal and in a voided season.
    #[test]
    fn claims_use_up_outstanding_exactly() {
        use crate::finalize::{finalize, void, Roster};
        use permutation_rules::genesis::{new_season, Entry};
        use permutation_rules::gov::{join, Path};
        use permutation_rules::{Preset, Ruleset};
        let rules = Ruleset::new(Preset::Blitz);
        let (fee, dep) = (10_000_000u64, 3_333_333u64);
        let sizes = [1usize, 2, 3];
        let entries: Vec<Entry> = sizes
            .iter()
            .enumerate()
            .map(|(c, n)| Entry {
                name: format!("n{c}"),
                treasury: dep * *n as u64,
            })
            .collect();
        let mut w = new_season(&rules, &[1; 32], &[2; 32], &entries).unwrap();
        let mut members = vec![];
        for (c, n) in sizes.iter().enumerate() {
            for _ in 0..*n {
                let k = members.len();
                join(&mut w, &rules, c as u16, [k as u8 + 1; 32]).unwrap();
                members.push(member(k as u32, c as u16, dep));
            }
        }
        for m in &mut w.members {
            m.windows = (1 << 18) - 1;
            m.merit[Path::Science as usize] = 1_000;
        }
        // Money moved between the nations: nobody's `left` divides evenly.
        w.civs[0].usdc = 1_000_001;
        w.civs[1].usdc = 7_000_003;
        w.civs[2].usdc = 3 * dep + 2 * dep + dep - 8_000_004;
        w.tick = rules.ticks_per_season;
        let ops = 6 * fee / 5;
        let mut season =
            finalized_season(entries.iter().map(|e| e.treasury).collect(), vec![], vec![]);
        (season.entry_fee, season.deposit, season.member_count) = (fee, dep, 6);
        (season.pool, season.ops, season.nations) = (6 * fee - ops, ops, 3);
        season.status = SeasonStatus::Running;
        let normal = finalize(&season, &w, &rules, Roster::None, false);
        let voided = void(&season, vec![]);
        assert!(!normal.voided);
        for f in [normal, voided] {
            let mut s = season.clone();
            s.status = SeasonStatus::Finalized;
            (s.payouts, s.treasury_final) = (f.payouts.clone(), f.treasury_final.clone());
            (s.refund_base, s.refund_in_payout) =
                (f.refund_base.clone(), f.refund_in_payout.clone());
            (s.ops, s.outstanding) = (f.ops, f.owed);
            let mut left = s.outstanding;
            for m in &members {
                left = left
                    .checked_sub(claim_amount(&s, m))
                    .expect("never above outstanding");
            }
            assert_eq!(
                left, s.ops,
                "voided {}: what stays is the operations share",
                f.voided
            );
        }
    }
}
