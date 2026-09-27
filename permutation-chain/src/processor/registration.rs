//! Base layer, before the season: the season account, world and nation
//! allocation, and membership.

use permutation_rules::fixed::BPS_ONE;
use permutation_rules::gov::NOBODY;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar::Sysvar,
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
    let prev_info = if prev_season_id == 0 {
        None
    } else {
        Some(next_account_info(it)?)
    };
    signer(admin)?;
    // Only seasons whose genesis was measured on the SBF build (WP13).
    let rules = rules_for(preset, market)?;
    if !creatable(preset, nations) || nations > rules.max_civs {
        msg!(
            "PS season preset {} with {} nations cannot be created",
            preset,
            nations
        );
        return Err(ChainError::InvalidParams.into());
    }
    // Every stage has a deadline (WP14): a tick is at most 4 h, and
    // StartSeason is due within the registration window.
    let now = now()?;
    if tick_seconds == 0
        || tick_seconds > MAX_TICK_SECONDS
        || start_by <= now
        || start_by > now.saturating_add(MAX_REGISTRATION_SECONDS)
        || validator == [0; 32]
    {
        return Err(ChainError::InvalidParams.into());
    }
    if token_program.key != &token::TOKEN_PROGRAM_ID {
        return Err(ProgramError::IncorrectProgramId);
    }
    let decimals = token::mint_decimals(mint).ok_or(ChainError::WrongMint)?;
    // The rules' USDC amounts are in 6-decimal units. A freeze authority
    // could freeze the vault and every Claim with it, so the mint has none;
    // the `mainnet` build admits Circle USDC only, whose freeze authority is
    // a documented trust root (WP12, WP15).
    let pinned = token::is_pinned_usdc(mint.key);
    if decimals != USDC_DECIMALS
        || (cfg!(feature = "mainnet") && !pinned)
        || (!pinned && token::mint_has_freeze_authority(mint) != Some(false))
    {
        return Err(ChainError::WrongMint.into());
    }
    if entry_fee > MAX_ENTRY_FEE
        || deposit > MAX_DEPOSIT
        || (deposit > 0 && !market)
        || bounty_each > MAX_BOUNTY
        || bond > MAX_BOND
    {
        return Err(ChainError::InvalidParams.into());
    }
    // At least one seat is left for people (WP07).
    if ai_count > MAX_AI || ai_count as u32 >= SEASON_MEMBER_CAP {
        return Err(ChainError::InvalidParams.into());
    }
    // The history layer: follow a finalized (or aborted) season of the same
    // admin, written by this program or the previous one.
    let prev_history_root = match prev_info {
        None => [0; 32],
        Some(prev_info) => prev_root(program_id, prev_info, prev_season_id, season_id, admin)?,
    };
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
        stage_at: now,
        rolled_back: 0,
        aborted_from: 0,
        validator,
        // Bound to the rules and settlement logic of this build (WP15).
        rules_version: crate::rules::PINNED_RULES_VERSION,
        rules_hash: crate::rules::pinned_ruleset_hash(preset, market)?,
        logic_version: crate::rules::CHAIN_LOGIC_VERSION,
        created_slot: Clock::get()?.slot,
    };
    store(&mut season_info.try_borrow_mut_data()?, &season)?;
    msg!(
        "PS season {} created: preset {} nations {} fee {} deposit {} market {} rules v{} logic v{}",
        season_id,
        preset,
        nations,
        entry_fee,
        deposit,
        market,
        season.rules_version,
        season.logic_version
    );
    Ok(())
}

/// The history root of CreateSeason's predecessor (account 6): a season of
/// this program or of the previous one (`load_season_compat`), Finalized or
/// Aborted, of the same admin, with another id. Not inlined: the second
/// Season stays out of `create_season`'s frame.
#[inline(never)]
fn prev_root(
    program_id: &Pubkey,
    prev_info: &AccountInfo,
    prev_season_id: u64,
    season_id: u64,
    admin: &AccountInfo,
) -> Result<[u8; 32], ProgramError> {
    let (prev, _legacy) = load_season_compat(program_id, prev_info)?;
    let done = matches!(prev.status, SeasonStatus::Finalized | SeasonStatus::Aborted);
    if prev.season_id != prev_season_id
        || !done
        || prev.admin != admin.key.to_bytes()
        || prev_season_id == season_id
    {
        return Err(ChainError::InvalidParams.into());
    }
    Ok(prev.history_root)
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
    // Only while registering: a season that moved on (or was aborted and
    // closed) never gets its accounts back (WP14).
    if season.status != SeasonStatus::Registering {
        return Err(ChainError::WrongStatus.into());
    }
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
    if season.status != SeasonStatus::Registering {
        return Err(ChainError::WrongStatus.into());
    }
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

/// Whether a registration may take a seat: the season and the nation
/// have room (`SEASON_MEMBER_CAP`, `NATION_MEMBER_CAP`), and in a season
/// with operator AI members the operator (admin or crank) pays for it.
/// Every join of such a season goes through the gateway, which admits each
/// seat counting the AI members still to come, so nobody can fill the
/// season past them (V5 §18.2); the payer tells nothing about who is an AI,
/// since the crank pays for both (WP03, WP07).
pub(super) fn open_seat(season: &Season, civ: u16, fee_payer: &[u8]) -> Result<(), ChainError> {
    if season.member_count >= SEASON_MEMBER_CAP
        || season.nation_members[civ as usize] >= NATION_MEMBER_CAP
    {
        return Err(ChainError::SeasonFull);
    }
    if season.ai_count > 0 && fee_payer != season.admin && fee_payer != season.crank {
        return Err(ChainError::Unauthorized);
    }
    Ok(())
}

/// First-election votes: each is `NOBODY` or a member index the Season can
/// hold (WP06).
pub(crate) fn check_votes(votes: &[u32; 4]) -> ProgramResult {
    if votes.iter().all(|v| *v == NOBODY || *v < MAX_MEMBERS) {
        Ok(())
    } else {
        Err(ChainError::InvalidParams.into())
    }
}

/// Seasons with operator AI members take no pre-season votes (V5 §18.14,
/// WP10): the first election is drawn among the candidates, for everyone.
fn check_ai_votes(season: &Season, votes: &[u32; 4]) -> ProgramResult {
    if season.ai_count > 0 && votes.iter().any(|v| *v != NOBODY) {
        return Err(ChainError::InvalidParams.into());
    }
    Ok(())
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
    // The session key signs its own registration (WP17), so nobody can bind
    // a key they do not hold (one copied from a Register in flight, or
    // published in an earlier season) to their membership.
    let session_info = next_account_info(it)?;
    signer(wallet)?;
    signer(fee_payer)?;
    if !session_info.is_signer || session_info.key.to_bytes() != session {
        return Err(ChainError::MissingSignature.into());
    }
    // A relayer that only pays the fee (the x402 facilitator) never becomes
    // a member's key; a wallet paying for itself may be its own session key.
    if session_info.key == fee_payer.key && fee_payer.key != wallet.key {
        return Err(ChainError::Unauthorized.into());
    }
    let mut season = load_season(program_id, season_info)?;
    if season.status != SeasonStatus::Registering {
        return Err(ChainError::WrongStatus.into());
    }
    require_rules(&season)?;
    if civ >= season.nations as u16 {
        return Err(ChainError::InvalidParams.into());
    }
    open_seat(&season, civ, fee_payer.key.as_ref())?;
    if name.is_empty() || name.len() > MAX_NAME || kind > MAX_KIND || stand > STAND_MASK {
        return Err(ChainError::InvalidName.into());
    }
    check_votes(&votes)?;
    check_ai_votes(&season, &votes)?;
    // One deposit per season (WP12); 0 when the market is off.
    if deposit != season.deposit {
        return Err(ChainError::InvalidParams.into());
    }
    check_usdc(program_id, &season, mint, token_program, vault)?;
    // The wallet pays from its own account, never as someone's SPL delegate
    // (as in Claim).
    check_token_account(wallet_token, mint, Some(wallet.key))?;
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
    // Entry fees: 80% prize pool, 20% operations (V5 D10), in checked
    // arithmetic (WP08, WP12).
    let rules = rules_for(season.preset, season.market)?;
    let ops = (season.entry_fee as u128 * rules.ops_share_bps as u128 / BPS_ONE as u128) as u64;
    let bad = || ProgramError::from(ChainError::InvalidParams);
    season.ops = season.ops.checked_add(ops).ok_or_else(bad)?;
    season.pool = season
        .pool
        .checked_add(season.entry_fee - ops)
        .ok_or_else(bad)?;
    let t = &mut season.treasury[civ as usize];
    *t = t.checked_add(deposit).ok_or_else(bad)?;
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

/// `PostBond { amount }`: the operator adds to its bond while registering,
/// so it reaches `bond_floor` before `StartSeason` (WP09). Capped at
/// `MAX_BOND` (WP12), which the floor never exceeds under the caps.
pub(super) fn post_bond(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    amount: u64,
) -> ProgramResult {
    let it = &mut accounts.iter();
    let authority = next_account_info(it)?;
    let season_info = next_account_info(it)?;
    let source = next_account_info(it)?;
    let vault = next_account_info(it)?;
    let mint = next_account_info(it)?;
    let token_program = next_account_info(it)?;
    signer(authority)?;
    let mut season = load_season(program_id, season_info)?;
    require_operator(&season, authority)?;
    if season.status != SeasonStatus::Registering {
        return Err(ChainError::WrongStatus.into());
    }
    if season.ai_count == 0 || amount == 0 {
        return Err(ChainError::InvalidParams.into());
    }
    check_usdc(program_id, &season, mint, token_program, vault)?;
    check_token_account(source, mint, None)?;
    season.bond = season
        .bond
        .checked_add(amount)
        .filter(|b| *b <= MAX_BOND)
        .ok_or(ChainError::InvalidParams)?;
    token::transfer_checked(
        source,
        mint,
        vault,
        authority,
        amount,
        season.usdc_decimals,
        None,
    )?;
    store(&mut season_info.try_borrow_mut_data()?, &season)?;
    msg!(
        "PS season {} bond {} (floor {})",
        season.season_id,
        season.bond,
        bond_floor(&season)
    );
    Ok(())
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
    check_ai_votes(&season, &votes)?;
    let mut m = load_member(program_id, member_info, season.season_id)?;
    if who.key.as_ref() != m.wallet && who.key.as_ref() != m.session {
        return Err(ChainError::Unauthorized.into());
    }
    if stand > STAND_MASK {
        return Err(ChainError::InvalidParams.into());
    }
    check_votes(&votes)?;
    m.stand = stand;
    m.votes = votes;
    store(&mut member_info.try_borrow_mut_data()?, &m)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: u64 = 7;

    fn season(bump: u8, ai_count: u16) -> Season {
        Season {
            magic: SEASON_MAGIC,
            season_id: ID,
            bump,
            vault_bump: 0,
            admin: [1; 32],
            crank: [2; 32],
            usdc_mint: [3; 32],
            usdc_decimals: 6,
            preset: PRESET_BLITZ,
            nations: 2,
            entry_fee: 10,
            tick_seconds: 30,
            market: true,
            status: SeasonStatus::Registering,
            world_seed: [0; 32],
            season_seed: [0; 32],
            member_count: 0,
            nation_members: vec![0, 0],
            seated: 0,
            pool: 0,
            ops: 0,
            ops_withdrawn: false,
            treasury: vec![0, 0],
            treasury_final: vec![],
            payouts: vec![],
            final_root: [0; 32],
            prev_season_id: 0,
            prev_history_root: [0; 32],
            history_root: [0; 32],
            ai_count,
            roster_commit: [0; 32],
            bounty_each: 0,
            bond: 0,
            roster_acc: [0; 32],
            roster_revealed: 0,
            roster_outcome: 0,
            bounty_paid: vec![],
            delegated: 0,
            roster_blind: [0; 32],
            refund_base: vec![],
            refund_in_payout: vec![],
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
            validator: [5; 32],
            rules_version: crate::rules::PINNED_RULES_VERSION,
            rules_hash: crate::rules::pinned_ruleset_hash(PRESET_BLITZ, true).unwrap(),
            logic_version: crate::rules::CHAIN_LOGIC_VERSION,
            created_slot: 0,
        }
    }

    #[test]
    fn check_votes_takes_nobody_or_a_member_index() {
        assert!(check_votes(&[NOBODY; 4]).is_ok());
        assert!(check_votes(&[0, 255, NOBODY, 1]).is_ok());
        let bad: ProgramError = ChainError::InvalidParams.into();
        assert_eq!(
            check_votes(&[256, NOBODY, NOBODY, NOBODY]),
            Err(bad.clone())
        );
        assert_eq!(check_votes(&[NOBODY - 1, 0, 0, 0]), Err(bad));
    }

    /// With operator AI members only the operator pays for a seat; the caps
    /// hold for every payer (WP03, WP07).
    #[test]
    fn open_seat_is_operator_only_in_ai_seasons() {
        let mut s = season(0, 6);
        let (admin, crank, outsider) = ([1u8; 32], [2u8; 32], [9u8; 32]);
        assert_eq!(open_seat(&s, 0, &outsider), Err(ChainError::Unauthorized));
        for count in 0..SEASON_MEMBER_CAP {
            s.member_count = count;
            assert_eq!(open_seat(&s, (count % 2) as u16, &crank), Ok(()));
            assert_eq!(open_seat(&s, (count % 2) as u16, &admin), Ok(()));
        }
        s.member_count = SEASON_MEMBER_CAP;
        assert_eq!(open_seat(&s, 0, &crank), Err(ChainError::SeasonFull));
        s.member_count = 30;
        s.nation_members = vec![NATION_MEMBER_CAP, 6];
        assert_eq!(open_seat(&s, 0, &crank), Err(ChainError::SeasonFull));
        assert_eq!(open_seat(&s, 1, &crank), Ok(()));
        // Without AIs, any payer, up to the same caps.
        let mut s = season(0, 0);
        assert_eq!(open_seat(&s, 1, &outsider), Ok(()));
        s.member_count = SEASON_MEMBER_CAP - 1;
        assert_eq!(open_seat(&s, 1, &outsider), Ok(()));
        s.member_count = SEASON_MEMBER_CAP;
        assert_eq!(open_seat(&s, 1, &outsider), Err(ChainError::SeasonFull));
    }

    #[test]
    fn the_rules_and_the_chain_agree_on_the_member_cap() {
        for market in [false, true] {
            assert_eq!(
                rules_for(PRESET_BLITZ, market).unwrap().max_members,
                SEASON_MEMBER_CAP
            );
        }
    }

    /// Accounts of a Register call: wallet, fee payer, season, member,
    /// wallet token, vault, mint, token program, system, session.
    struct Reg {
        keys: Vec<Pubkey>,
        signers: Vec<bool>,
        owners: Vec<Pubkey>,
        data: Vec<Vec<u8>>,
        lamports: Vec<u64>,
    }

    impl Reg {
        const WALLET: usize = 0;
        const PAYER: usize = 1;
        const SEASON: usize = 2;
        const MEMBER: usize = 3;
        const TOKEN: usize = 4;
        const SESSION: usize = 9;

        /// A valid registration of a fresh wallet, self-paid, into a
        /// Registering season (whose account is a real PDA of `program`).
        fn new(program: &Pubkey) -> Reg {
            let wallet = Pubkey::new_unique();
            let session = Pubkey::new_unique();
            let (season_key, bump) =
                Pubkey::find_program_address(&[SEASON_SEED, &ID.to_le_bytes()], program);
            let (member, _) = Pubkey::find_program_address(
                &[MEMBER_SEED, &ID.to_le_bytes(), wallet.as_ref()],
                program,
            );
            let (vault, _) =
                Pubkey::find_program_address(&[VAULT_SEED, &ID.to_le_bytes()], program);
            let mint = Pubkey::new_from_array([3; 32]);
            let mut s = vec![0u8; SEASON_SPACE];
            store(&mut s, &season(bump, 0)).unwrap();
            let token = |owner: &Pubkey| {
                let mut d = vec![0u8; token::TOKEN_ACCOUNT_LEN];
                d[..32].copy_from_slice(mint.as_ref());
                d[32..64].copy_from_slice(owner.as_ref());
                d
            };
            let system = Pubkey::default();
            Reg {
                keys: vec![
                    wallet,
                    wallet,
                    season_key,
                    member,
                    Pubkey::new_unique(),
                    vault,
                    mint,
                    token::TOKEN_PROGRAM_ID,
                    system,
                    session,
                ],
                signers: vec![
                    true, true, false, false, false, false, false, false, false, true,
                ],
                owners: vec![
                    system,
                    system,
                    *program,
                    system,
                    token::TOKEN_PROGRAM_ID,
                    token::TOKEN_PROGRAM_ID,
                    token::TOKEN_PROGRAM_ID,
                    system,
                    system,
                    system,
                ],
                data: vec![
                    vec![],
                    vec![],
                    s,
                    vec![],
                    token(&wallet),
                    token(&season_key),
                    vec![0; token::MINT_LEN],
                    vec![],
                    vec![],
                    vec![],
                ],
                lamports: vec![0; 10],
            }
        }

        /// Register the wallet with `session` as the data's session key,
        /// passing the first `n` accounts.
        fn run(&mut self, program: &Pubkey, session: Pubkey, n: usize) -> ProgramResult {
            let Reg {
                keys,
                signers,
                owners,
                data,
                lamports,
            } = self;
            let infos: Vec<AccountInfo> = keys
                .iter()
                .zip(signers.iter())
                .zip(owners.iter())
                .zip(data.iter_mut())
                .zip(lamports.iter_mut())
                .map(|((((k, s), o), d), l)| AccountInfo::new(k, *s, true, l, d, o, false))
                .take(n)
                .collect();
            register(
                program,
                &infos,
                0,
                "m".into(),
                0,
                session.to_bytes(),
                [0; 32],
                0x0f,
                [NOBODY; 4],
                0,
                [0; 32],
            )
        }
    }

    const MISSING_SIGNATURE: u32 = ChainError::MissingSignature as u32;
    const UNAUTHORIZED: u32 = ChainError::Unauthorized as u32;

    #[test]
    fn register_needs_the_session_account() {
        let program = Pubkey::new_unique();
        let mut r = Reg::new(&program);
        let session = r.keys[Reg::SESSION];
        assert_eq!(
            r.run(&program, session, 9),
            Err(ProgramError::NotEnoughAccountKeys)
        );
    }

    #[test]
    fn register_refuses_a_session_key_that_did_not_sign() {
        let program = Pubkey::new_unique();
        let mut r = Reg::new(&program);
        let session = r.keys[Reg::SESSION];
        r.signers[Reg::SESSION] = false;
        assert_eq!(
            r.run(&program, session, 10),
            Err(ProgramError::Custom(MISSING_SIGNATURE))
        );
        // The front-runner signs with its own key, naming the victim's in
        // the data.
        r.signers[Reg::SESSION] = true;
        let victims = Pubkey::new_unique();
        assert_eq!(
            r.run(&program, victims, 10),
            Err(ProgramError::Custom(MISSING_SIGNATURE))
        );
    }

    #[test]
    fn register_refuses_the_relaying_fee_payer_as_session() {
        let program = Pubkey::new_unique();
        let mut r = Reg::new(&program);
        // A relayer F pays and names itself as the session key.
        let relayer = Pubkey::new_unique();
        r.keys[Reg::PAYER] = relayer;
        r.keys[Reg::SESSION] = relayer;
        assert_eq!(
            r.run(&program, relayer, 10),
            Err(ProgramError::Custom(UNAUTHORIZED))
        );
        // A wallet paying for itself may be its own session key: the
        // checks pass (the call then fails later, on the token account).
        let wallet = r.keys[Reg::WALLET];
        r.keys[Reg::PAYER] = wallet;
        r.keys[Reg::SESSION] = wallet;
        let e = r.run(&program, wallet, 10).unwrap_err();
        assert!(
            e != ProgramError::Custom(MISSING_SIGNATURE) && e != ProgramError::Custom(UNAUTHORIZED),
            "{e:?}"
        );
    }

    /// The fee source must be the wallet's own token account (WP17): an
    /// account the wallet may spend only as a delegate is refused.
    #[test]
    fn register_refuses_a_token_account_the_wallet_does_not_own() {
        let program = Pubkey::new_unique();
        let mut r = Reg::new(&program);
        let session = r.keys[Reg::SESSION];
        let other = Pubkey::new_unique();
        r.data[Reg::TOKEN][32..64].copy_from_slice(other.as_ref());
        assert_eq!(
            r.run(&program, session, 10),
            Err(ChainError::WrongTokenAccount.into())
        );
        // Owned by the wallet, it passes; the call fails later, at the
        // member PDA (another wallet's).
        let wallet = r.keys[Reg::WALLET];
        r.data[Reg::TOKEN][32..64].copy_from_slice(wallet.as_ref());
        r.keys[Reg::MEMBER] = Pubkey::new_unique();
        assert_eq!(
            r.run(&program, session, 10),
            Err(ChainError::WrongPda.into())
        );
    }

    /// The registration checks that need no CPI, in a season with AIs: a
    /// self-paid join is refused, a vote is refused, and a deposit other
    /// than the season's is refused.
    #[test]
    fn register_checks_before_any_cpi() {
        let program = Pubkey::new_unique();
        let mut r = Reg::new(&program);
        let session = r.keys[Reg::SESSION];
        let mut s: Season = load(&r.data[Reg::SEASON]).unwrap();
        s.ai_count = 1;
        store(&mut r.data[Reg::SEASON], &s).unwrap();
        assert_eq!(
            r.run(&program, session, 10),
            Err(ChainError::Unauthorized.into())
        );
        s.ai_count = 0;
        s.deposit = 5;
        store(&mut r.data[Reg::SEASON], &s).unwrap();
        assert_eq!(
            r.run(&program, session, 10),
            Err(ChainError::InvalidParams.into())
        );
        s.deposit = 0;
        s.rules_hash[0] ^= 1;
        store(&mut r.data[Reg::SEASON], &s).unwrap();
        assert_eq!(
            r.run(&program, session, 10),
            Err(ChainError::RulesMismatch.into())
        );
    }
}
