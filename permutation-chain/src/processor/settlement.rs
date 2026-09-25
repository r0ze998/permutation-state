//! Base layer, after the season: payouts, claims and the operations share.

use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};

use super::accounts::*;
use crate::error::ChainError;
use crate::state::*;
use crate::token;

pub(super) fn finish_season(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let season_ai = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let mut season = load_season(program_id, season_ai)?;
    if season.status != SeasonStatus::Running {
        return Err(ChainError::WrongStatus.into());
    }
    // Undelegated and back with its final state (world_chunks checks ownership).
    let world = world_chunks(program_id, &accounts[1..], season.season_id)?;
    if !world.is_world() {
        return Err(ChainError::WrongWorld.into());
    }
    let state = world.read_world()?;
    let rules = rules_for(season.preset, season.market)?;
    if state.tick < rules.ticks_per_season {
        return Err(ChainError::SeasonNotOver.into());
    }
    // In-play income (market fees and tariffs) joins the pool and operations
    // in the same 80/20 split as the fees (V5 D11).
    let pool = season.pool + state.exchange_vault;
    let s = permutation_rules::payout::settle(&state, &rules, pool, season.entry_fee);
    season.pool = pool;
    season.ops += state.exchange_ops + s.dust;
    season.payouts = s.per_member.clone();
    season.treasury_final = state.civs.iter().map(|c| c.usdc).collect();
    season.final_root = state.state_root().map_err(|_| ChainError::Rules)?;
    // The history layer: this season's record, chained onto the one it
    // follows, and logged in full for the next season and the verifier.
    let record = permutation_rules::history::season_record(&state, &s);
    season.history_root =
        permutation_rules::history::history_root(&season.prev_history_root, &record);
    let mut bytes = borsh::to_vec(&record).map_err(|_| ChainError::Rules)?;
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
    season.status = SeasonStatus::Finalized;
    store(&mut season_ai.try_borrow_mut_data()?, &season)?;
    msg!(
        "PS season {} finalized: pool {} ops {} refund {}",
        season.season_id,
        season.pool,
        season.ops,
        s.refund
    );
    Ok(())
}

/// What a member receives: its prize, plus its share of what is left in
/// its nation's treasury (V5 §7.5).
pub fn claim_amount(season: &Season, member: &MemberAccount) -> u64 {
    let prize = season
        .payouts
        .get(member.index as usize)
        .copied()
        .unwrap_or(0);
    let civ = member.civ as usize;
    let deposited = season.treasury.get(civ).copied().unwrap_or(0);
    let left = season.treasury_final.get(civ).copied().unwrap_or(0);
    let refund = if deposited == 0 {
        0
    } else {
        (member.shares as u128 * left as u128 / deposited as u128) as u64
    };
    prize + refund
}

pub(super) fn claim(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let it = &mut accounts.iter();
    let wallet = next_account_info(it)?;
    let season_ai = next_account_info(it)?;
    let member_ai = next_account_info(it)?;
    let vault = next_account_info(it)?;
    let dest = next_account_info(it)?;
    let mint = next_account_info(it)?;
    let token_program = next_account_info(it)?;
    signer(wallet)?;
    let season = load_season(program_id, season_ai)?;
    if season.status != SeasonStatus::Finalized {
        return Err(ChainError::WrongStatus.into());
    }
    let mut member = load_member(program_id, member_ai, season.season_id)?;
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
    let id = season.season_id.to_le_bytes();
    let (dmint, downer) =
        token::token_account_mint_owner(dest).ok_or(ChainError::WrongTokenAccount)?;
    if dmint != *mint.key || downer != *wallet.key {
        return Err(ChainError::WrongTokenAccount.into());
    }
    member.claimed = true;
    store(&mut member_ai.try_borrow_mut_data()?, &member)?;
    token::transfer_checked(
        vault,
        mint,
        dest,
        season_ai,
        amount,
        season.usdc_decimals,
        Some(&[SEASON_SEED, &id, &[season.bump]]),
    )?;
    msg!(
        "PS claim season {} member {} amount {}",
        season.season_id,
        member.index,
        amount
    );
    Ok(())
}

pub(super) fn withdraw_ops(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let it = &mut accounts.iter();
    let admin = next_account_info(it)?;
    let season_ai = next_account_info(it)?;
    let vault = next_account_info(it)?;
    let dest = next_account_info(it)?;
    let mint = next_account_info(it)?;
    let token_program = next_account_info(it)?;
    signer(admin)?;
    let mut season = load_season(program_id, season_ai)?;
    if admin.key.as_ref() != season.admin {
        return Err(ChainError::Unauthorized.into());
    }
    if season.status != SeasonStatus::Finalized || season.ops_withdrawn {
        return Err(ChainError::WrongStatus.into());
    }
    check_usdc(program_id, &season, mint, token_program, vault)?;
    let id = season.season_id.to_le_bytes();
    let (dmint, _) = token::token_account_mint_owner(dest).ok_or(ChainError::WrongTokenAccount)?;
    if dmint != *mint.key {
        return Err(ChainError::WrongTokenAccount.into());
    }
    season.ops_withdrawn = true;
    let amount = season.ops;
    store(&mut season_ai.try_borrow_mut_data()?, &season)?;
    if amount > 0 {
        token::transfer_checked(
            vault,
            mint,
            dest,
            season_ai,
            amount,
            season.usdc_decimals,
            Some(&[SEASON_SEED, &id, &[season.bump]]),
        )?;
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
        }
    }

    /// Prizes plus pro-rata treasury refunds never pay out more than the
    /// treasury has left.
    #[test]
    fn claims_split_the_treasury_by_shares() {
        let season = Season {
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
            treasury: vec![300, 0],
            treasury_final: vec![100, 0],
            payouts: vec![5, 6, 7],
            final_root: [0; 32],
            prev_season_id: 0,
            prev_history_root: [0; 32],
            history_root: [0; 32],
        };
        let (a, b, c) = (member(0, 0, 200), member(1, 0, 100), member(2, 1, 0));
        assert_eq!(claim_amount(&season, &a), 5 + 66);
        assert_eq!(claim_amount(&season, &b), 6 + 33);
        assert_eq!(claim_amount(&season, &c), 7);
        let out_of_range = member(9, 5, 10);
        assert_eq!(claim_amount(&season, &out_of_range), 0);
    }
}
