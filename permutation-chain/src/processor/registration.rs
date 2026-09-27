//! Base layer, before the season: the season account, world and nation
//! allocation, and membership.

use permutation_rules::fixed::BPS_ONE;
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

/// `CreateSeason`'s parameters besides the id.
pub(super) struct SeasonParams {
    pub preset: u8,
    pub nations: u8,
    pub entry_fee: u64,
    pub tick_seconds: u32,
    pub world_seed: [u8; 32],
    pub crank: [u8; 32],
    pub market: bool,
    pub prev_season_id: u64,
    pub ai_count: u16,
    pub roster_commit: [u8; 32],
    pub bounty_each: u64,
    pub bond: u64,
    pub deposit: u64,
    pub start_by: i64,
    pub validator: [u8; 32],
}

pub(super) fn create_season(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    season_id: u64,
    p: SeasonParams,
) -> ProgramResult {
    let SeasonParams {
        preset,
        nations,
        entry_fee,
        tick_seconds,
        world_seed,
        crank,
        market,
        prev_season_id,
        ai_count,
        roster_commit,
        bounty_each,
        bond,
        deposit,
        start_by,
        validator,
    } = p;
    let it = &mut accounts.iter();
    let admin = next_account_info(it)?;
    let season_info = next_account_info(it)?;
    let vault = next_account_info(it)?;
    let mint = next_account_info(it)?;
    let token_program = next_account_info(it)?;
    let _system = next_account_info(it)?;
    signer(admin)?;
    // The history layer: follow a finalized season of the same admin.
    let prev_history_root = if prev_season_id == 0 {
        [0; 32]
    } else {
        let prev_info = next_account_info(it)?;
        let prev = load_season(program_id, prev_info)?;
        if prev.season_id != prev_season_id
            || prev.status != SeasonStatus::Finalized
            || prev.admin != admin.key.to_bytes()
            || prev_season_id == season_id
        {
            return Err(ChainError::InvalidParams.into());
        }
        prev.history_root
    };
    let rules = rules_for(preset, market)?;
    let max = (rules.max_civs as usize)
        .min(MAX_NATIONS)
        .min(permutation_rules::genesis::NATIONS.len());
    if !(2..=max as u8).contains(&nations) || tick_seconds == 0 {
        return Err(ChainError::InvalidParams.into());
    }
    if token_program.key != &token::TOKEN_PROGRAM_ID {
        return Err(ProgramError::IncorrectProgramId);
    }
    let decimals = token::mint_decimals(mint).ok_or(ChainError::WrongMint)?;
    let id = season_id.to_le_bytes();
    let bump = expect_pda(program_id, season_info, &season_seeds(&id))?;
    let vault_bump = expect_pda(program_id, vault, &[VAULT_SEED, &id])?;
    init_pda(
        admin,
        season_info,
        SEASON_SPACE,
        program_id,
        &[SEASON_SEED, &id, &[bump]],
    )?;
    init_pda(
        admin,
        vault,
        token::TOKEN_ACCOUNT_LEN,
        &token::TOKEN_PROGRAM_ID,
        &[VAULT_SEED, &id, &[vault_bump]],
    )?;
    token::initialize_account3(vault, mint, season_info.key)?;
    // Operator AI members (V5 §18): escrow their bounties and the bond, and
    // create the roster their salts are revealed into after the season.
    if ai_count > MAX_AI {
        return Err(ChainError::InvalidParams.into());
    }
    if ai_count > 0 {
        let admin_token = next_account_info(it)?;
        let roster_info = next_account_info(it)?;
        let escrow = bounty_each
            .checked_mul(ai_count as u64)
            .and_then(|x| x.checked_add(bond))
            .ok_or(ChainError::InvalidParams)?;
        if escrow > 0 {
            token::transfer_checked(admin_token, mint, vault, admin, escrow, decimals, None)?;
        }
        let roster_bump = expect_pda(program_id, roster_info, &[ROSTER_SEED, &id])?;
        init_pda(
            admin,
            roster_info,
            roster_space(ai_count),
            program_id,
            &[ROSTER_SEED, &id, &[roster_bump]],
        )?;
        let roster = RosterAccount {
            magic: ROSTER_MAGIC,
            season_id,
            bump: roster_bump,
            entries: Vec::new(),
        };
        store(&mut roster_info.try_borrow_mut_data()?, &roster)?;
    } else if bounty_each != 0 || bond != 0 || roster_commit != [0; 32] {
        return Err(ChainError::InvalidParams.into());
    }
    let season = Season {
        magic: SEASON_MAGIC,
        season_id,
        bump,
        vault_bump,
        admin: admin.key.to_bytes(),
        crank,
        usdc_mint: mint.key.to_bytes(),
        usdc_decimals: decimals,
        preset,
        nations,
        entry_fee,
        tick_seconds,
        market,
        status: SeasonStatus::Registering,
        world_seed,
        season_seed: [0; 32],
        member_count: 0,
        nation_members: vec![0; nations as usize],
        seated: 0,
        pool: 0,
        ops: 0,
        ops_withdrawn: false,
        treasury: vec![0; nations as usize],
        treasury_final: Vec::new(),
        payouts: Vec::new(),
        final_root: [0; 32],
        prev_season_id,
        prev_history_root,
        history_root: [0; 32],
        ai_count,
        roster_commit,
        bounty_each,
        bond,
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
        deposit,
        outstanding: 0,
        voided: false,
        start_by,
        stage_at: 0,
        rolled_back: 0,
        aborted_from: 0,
        validator,
        rules_version: crate::rules::PINNED_RULES_VERSION,
        rules_hash: crate::rules::pinned_ruleset_hash(preset, market)?,
        logic_version: crate::rules::CHAIN_LOGIC_VERSION,
        created_slot: 0,
    };
    store(&mut season_info.try_borrow_mut_data()?, &season)?;
    msg!(
        "PS season {} created: preset {} nations {} fee {} market {}",
        season_id,
        preset,
        nations,
        entry_fee,
        market
    );
    Ok(())
}

pub(super) fn alloc_world(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    chunk: u8,
) -> ProgramResult {
    let it = &mut accounts.iter();
    let payer = next_account_info(it)?;
    let season_info = next_account_info(it)?;
    let world = next_account_info(it)?;
    let _system = next_account_info(it)?;
    signer(payer)?;
    let season = load_season(program_id, season_info)?;
    if chunk as usize >= WORLD_CHUNKS {
        return Err(ChainError::InvalidParams.into());
    }
    let id = season.season_id.to_le_bytes();
    let bump = expect_pda(program_id, world, &[WORLD_SEED, &id, &[chunk]])?;
    init_pda(
        payer,
        world,
        CHUNK,
        program_id,
        &[WORLD_SEED, &id, &[chunk], &[bump]],
    )
}

pub(super) fn alloc_nation(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    civ: u16,
) -> ProgramResult {
    let it = &mut accounts.iter();
    let payer = next_account_info(it)?;
    let season_info = next_account_info(it)?;
    let nation_info = next_account_info(it)?;
    let _system = next_account_info(it)?;
    signer(payer)?;
    let season = load_season(program_id, season_info)?;
    if civ >= season.nations as u16 {
        return Err(ChainError::InvalidParams.into());
    }
    let id = season.season_id.to_le_bytes();
    let civ_bytes = civ.to_le_bytes();
    let bump = expect_pda(program_id, nation_info, &[NATION_SEED, &id, &civ_bytes])?;
    init_pda(
        payer,
        nation_info,
        NATION_SPACE,
        program_id,
        &[NATION_SEED, &id, &civ_bytes, &[bump]],
    )?;
    let n = NationAccount::new(
        season.season_id,
        civ,
        bump,
        season.preset,
        season.market,
        season.crank,
    );
    store(&mut nation_info.try_borrow_mut_data()?, &n)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn register(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    civ: u16,
    name: String,
    kind: u8,
    session: [u8; 32],
    attestation: [u8; 32],
    stand: u8,
    votes: [u32; 4],
    deposit: u64,
    tag: [u8; 32],
) -> ProgramResult {
    let it = &mut accounts.iter();
    let wallet = next_account_info(it)?;
    let fee_payer = next_account_info(it)?;
    let season_info = next_account_info(it)?;
    let member_info = next_account_info(it)?;
    let wallet_token = next_account_info(it)?;
    let vault = next_account_info(it)?;
    let mint = next_account_info(it)?;
    let token_program = next_account_info(it)?;
    let _system = next_account_info(it)?;
    signer(wallet)?;
    signer(fee_payer)?;
    let mut season = load_season(program_id, season_info)?;
    if season.status != SeasonStatus::Registering {
        return Err(ChainError::WrongStatus.into());
    }
    if civ >= season.nations as u16 {
        return Err(ChainError::InvalidParams.into());
    }
    if season.member_count >= MAX_MEMBERS {
        return Err(ChainError::SeasonFull.into());
    }
    if name.is_empty() || name.len() > MAX_NAME || kind > MAX_KIND || stand > STAND_MASK {
        return Err(ChainError::InvalidName.into());
    }
    if deposit > 0 && !season.market {
        return Err(ChainError::InvalidParams.into());
    }
    check_usdc(program_id, &season, mint, token_program, vault)?;
    check_token_account(wallet_token, mint, None)?;
    let id = season.season_id.to_le_bytes();
    // One member per wallet per season (V5 D1): the PDA is keyed by the wallet.
    let bump = expect_pda(
        program_id,
        member_info,
        &[MEMBER_SEED, &id, wallet.key.as_ref()],
    )?;
    init_pda(
        fee_payer,
        member_info,
        MEMBER_SPACE,
        program_id,
        &[MEMBER_SEED, &id, wallet.key.as_ref(), &[bump]],
    )?;
    let member = MemberAccount {
        magic: MEMBER_MAGIC,
        season_id: season.season_id,
        bump,
        index: season.member_count,
        civ,
        wallet: wallet.key.to_bytes(),
        session,
        kind,
        name,
        attestation,
        stand,
        votes,
        shares: deposit,
        claimed: false,
        tag,
    };
    store(&mut member_info.try_borrow_mut_data()?, &member)?;
    let total = season
        .entry_fee
        .checked_add(deposit)
        .ok_or(ChainError::InvalidParams)?;
    if total > 0 {
        token::transfer_checked(
            wallet_token,
            mint,
            vault,
            wallet,
            total,
            season.usdc_decimals,
            None,
        )?;
    }
    // Entry fees: 80% prize pool, 20% operations (V5 D10).
    let rules = rules_for(season.preset, season.market)?;
    let ops = season.entry_fee * rules.ops_share_bps as u64 / BPS_ONE as u64;
    season.ops += ops;
    season.pool += season.entry_fee - ops;
    season.treasury[civ as usize] += deposit;
    season.nation_members[civ as usize] += 1;
    season.member_count += 1;
    store(&mut season_info.try_borrow_mut_data()?, &season)?;
    msg!(
        "PS member {} joined nation {} season {} pool {}",
        member.index,
        civ,
        season.season_id,
        season.pool
    );
    Ok(())
}

/// `PostBond { amount }` (lands with unit P2; refused until then).
pub(super) fn post_bond(
    _program_id: &Pubkey,
    _accounts: &[AccountInfo],
    _amount: u64,
) -> ProgramResult {
    Err(ChainError::InvalidInstruction.into())
}

pub(super) fn update_member(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    stand: u8,
    votes: [u32; 4],
) -> ProgramResult {
    let it = &mut accounts.iter();
    let who = next_account_info(it)?;
    let season_info = next_account_info(it)?;
    let member_info = next_account_info(it)?;
    signer(who)?;
    let season = load_season(program_id, season_info)?;
    if season.status != SeasonStatus::Registering {
        return Err(ChainError::WrongStatus.into());
    }
    let mut m = load_member(program_id, member_info, season.season_id)?;
    if who.key.as_ref() != m.wallet && who.key.as_ref() != m.session {
        return Err(ChainError::Unauthorized.into());
    }
    if stand > STAND_MASK {
        return Err(ChainError::InvalidParams.into());
    }
    m.stand = stand;
    m.votes = votes;
    store(&mut member_info.try_borrow_mut_data()?, &m)
}
