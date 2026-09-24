use borsh::BorshDeserialize;
use ephemeral_rollups_sdk::{
    consts::{DELEGATION_PROGRAM_ID, MAGIC_CONTEXT_ID, MAGIC_PROGRAM_ID},
    cpi::{delegate_account, undelegate_account, DelegateAccounts, DelegateConfig},
    ephem::{FoldableIntentBuilder, MagicIntentBundleBuilder},
};
use permutation_rules::genesis::{check_entries, nation_entries, season_from_map};
use permutation_rules::gov::{self, GovAction, GovEntry, Role, NOBODY};
use permutation_rules::map::{MapJob, MapStep};
use permutation_rules::orders::{check_structure, role_allows_static, Order, OrderBatch};
use permutation_rules::state::WorldState;
use permutation_rules::tick::{run_phase, TickInput};
use permutation_rules::{Preset, Ruleset};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint::ProgramResult,
    hash::hashv,
    msg,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    sysvar::{slot_hashes, Sysvar},
};
use solana_system_interface::{instruction as system_instruction, program as system_program};

use crate::error::ChainError;
use crate::instruction::ChainInstruction;
use crate::state::*;
use crate::token;

/// Fixed discriminator the delegation program calls on undelegation.
pub const UNDELEGATE_CALLBACK_DISCRIMINATOR: [u8; 8] = [196, 28, 41, 206, 48, 37, 51, 167];
/// `Delegate { target }`: world chunks are 0..WORLD_CHUNKS, nations are NATION_TARGET + civ.
pub const NATION_TARGET: u16 = 1000;
/// How often the ER auto-commits delegated accounts to the base layer.
pub const ER_COMMIT_FREQUENCY_MS: u32 = 30_000;

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    if let Some(seeds) = data.strip_prefix(&UNDELEGATE_CALLBACK_DISCRIMINATOR) {
        let seeds = Vec::<Vec<u8>>::try_from_slice(seeds).map_err(|_| ChainError::InvalidInstruction)?;
        return undelegate_callback(program_id, accounts, seeds);
    }
    let ix = ChainInstruction::try_from_slice(data).map_err(|_| ChainError::InvalidInstruction)?;
    match ix {
        ChainInstruction::CreateSeason { season_id, preset, nations, entry_fee, tick_seconds, world_seed, crank, market } => {
            create_season(program_id, accounts, season_id, preset, nations, entry_fee, tick_seconds, world_seed, crank, market)
        }
        ChainInstruction::AllocWorld { chunk } => alloc_world(program_id, accounts, chunk),
        ChainInstruction::Register { civ, name, kind, session, attestation, stand, votes, deposit } => {
            register(program_id, accounts, civ, name, kind, session, attestation, stand, votes, deposit)
        }
        ChainInstruction::StartSeason => start_season(program_id, accounts),
        ChainInstruction::GenesisStep { work } => genesis_step(program_id, accounts, work),
        ChainInstruction::Delegate { target } => delegate(program_id, accounts, target),
        ChainInstruction::SubmitOrders { role, tick, decision_digest, orders, adopt } => {
            submit_orders(program_id, accounts, role, tick, decision_digest, orders, adopt)
        }
        ChainInstruction::ResolveTick { to } => resolve_tick(program_id, accounts, to),
        ChainInstruction::Commit => commit(program_id, accounts, false),
        ChainInstruction::CommitAndUndelegate => commit(program_id, accounts, true),
        ChainInstruction::FinishSeason => finish_season(program_id, accounts),
        ChainInstruction::Claim => claim(program_id, accounts),
        ChainInstruction::UndelegatePart { targets } => undelegate_part(program_id, accounts, targets),
        ChainInstruction::UpdateMember { stand, votes } => update_member(program_id, accounts, stand, votes),
        ChainInstruction::AllocNation { civ } => alloc_nation(program_id, accounts, civ),
        ChainInstruction::SeatMembers => seat_members(program_id, accounts),
        ChainInstruction::OpenGovernment => open_government(program_id, accounts),
        ChainInstruction::SubmitGov { member, action } => submit_gov(program_id, accounts, member, action),
        ChainInstruction::WithdrawOps => withdraw_ops(program_id, accounts),
        ChainInstruction::LogTickInput { chunk } => log_tick_input(program_id, accounts, chunk),
    }
}

// ------------------------------------------------------------------ helpers

/// The season's ruleset: the preset, with the market switched on or off (V5 §7.5).
fn rules_for(preset: u8, market: bool) -> Result<Ruleset, ProgramError> {
    let mut rules = match preset {
        0 => Ruleset::new(Preset::Blitz),
        1 => Ruleset::new(Preset::Season),
        _ => return Err(ChainError::InvalidParams.into()),
    };
    rules.market_enabled = market;
    Ok(rules)
}

fn signer(a: &AccountInfo) -> ProgramResult {
    if a.is_signer {
        Ok(())
    } else {
        Err(ChainError::MissingSignature.into())
    }
}

fn expect_pda(program_id: &Pubkey, account: &AccountInfo, seeds: &[&[u8]]) -> Result<u8, ProgramError> {
    let (pda, bump) = Pubkey::find_program_address(seeds, program_id);
    if pda != *account.key {
        return Err(ChainError::WrongPda.into());
    }
    Ok(bump)
}

/// Claim a system-owned PDA even if a third party pre-funded it (defeats the
/// one-lamport squatting attack), then assign it to `owner`.
fn init_pda<'a>(payer: &AccountInfo<'a>, pda: &AccountInfo<'a>, space: usize, owner: &Pubkey, seeds: &[&[u8]]) -> ProgramResult {
    if pda.owner != &system_program::id() || !pda.data_is_empty() {
        return Err(ChainError::AlreadyInitialized.into());
    }
    let need = Rent::get()?.minimum_balance(space).saturating_sub(pda.lamports());
    if need > 0 {
        invoke(&system_instruction::transfer(payer.key, pda.key, need), &[payer.clone(), pda.clone()])?;
    }
    invoke_signed(&system_instruction::allocate(pda.key, space as u64), std::slice::from_ref(pda), &[seeds])?;
    invoke_signed(&system_instruction::assign(pda.key, owner), std::slice::from_ref(pda), &[seeds])
}

fn season_seeds(id: &[u8; 8]) -> [&[u8]; 2] {
    [SEASON_SEED, id]
}

fn load_season(program_id: &Pubkey, account: &AccountInfo) -> Result<Season, ProgramError> {
    if account.owner != program_id {
        return Err(ChainError::NotInitialized.into());
    }
    let s: Season = load(&account.try_borrow_data()?)?;
    if s.magic != SEASON_MAGIC {
        return Err(ChainError::NotInitialized.into());
    }
    expect_pda(program_id, account, &season_seeds(&s.season_id.to_le_bytes()))?;
    Ok(s)
}

fn load_nation(program_id: &Pubkey, account: &AccountInfo, season_id: u64, civ: u16) -> Result<NationAccount, ProgramError> {
    if account.owner != program_id {
        return Err(ChainError::MissingNation.into());
    }
    let n: NationAccount = load(&account.try_borrow_data()?)?;
    if n.magic != NATION_MAGIC || n.season_id != season_id || n.civ != civ {
        return Err(ChainError::MissingNation.into());
    }
    Ok(n)
}

fn load_member(program_id: &Pubkey, account: &AccountInfo, season_id: u64) -> Result<MemberAccount, ProgramError> {
    if account.owner != program_id {
        return Err(ChainError::NotInitialized.into());
    }
    let m: MemberAccount = load(&account.try_borrow_data()?)?;
    if m.magic != MEMBER_MAGIC || m.season_id != season_id {
        return Err(ChainError::NotInitialized.into());
    }
    expect_pda(program_id, account, &[MEMBER_SEED, &season_id.to_le_bytes(), &m.wallet])?;
    Ok(m)
}

/// Refresh a nation account for the open tick from the world: office
/// holders, their keys and budgets; clear the batches and the inbox.
fn open_nation(n: &mut NationAccount, state: &WorldState, rules: &Ruleset) {
    let civ = n.civ as usize;
    let nation = &state.nations[civ];
    n.open_tick = state.tick;
    for role in Role::ALL {
        let i = role.index();
        let m = nation.offices[i];
        n.officers[i] = m;
        n.keys[i] = state.members.get(m as usize).map_or([0; 32], |x| x.key);
        n.spendable[i] = permutation_rules::orders::spendable(state, rules, civ as u16, role);
    }
    n.batches = [None, None, None, None];
    n.submitted = [u16::MAX; 4];
    n.frozen = false;
    n.inbox.clear();
}

/// The `WORLD_CHUNKS` world accounts at the front of `accounts`, checked.
fn world_chunks<'a, 'info>(program_id: &Pubkey, accounts: &'a [AccountInfo<'info>], season_id: u64) -> Result<Chunks<'a, 'info>, ProgramError> {
    let list = accounts.get(..WORLD_CHUNKS).ok_or(ChainError::WorldTooSmall)?;
    let id = season_id.to_le_bytes();
    for (k, a) in list.iter().enumerate() {
        if a.owner != program_id {
            return Err(ChainError::WrongWorld.into());
        }
        expect_pda(program_id, a, &[WORLD_SEED, &id, &[k as u8]])?;
    }
    Chunks::new(list)
}

/// Season id recorded in world chunk 0 (to find the other accounts).
fn world_season_id(chunk0: &AccountInfo) -> Result<u64, ProgramError> {
    let d = chunk0.try_borrow_data()?;
    Ok(u64::from_le_bytes(d.get(12..20).ok_or(ChainError::WorldTooSmall)?.try_into().unwrap()))
}

fn is_operator(season: &Season, key: &Pubkey) -> bool {
    key.as_ref() == season.admin || key.as_ref() == season.crank
}

// ------------------------------------------------------------------ base: season setup

#[allow(clippy::too_many_arguments)]
fn create_season(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    season_id: u64,
    preset: u8,
    nations: u8,
    entry_fee: u64,
    tick_seconds: u32,
    world_seed: [u8; 32],
    crank: [u8; 32],
    market: bool,
) -> ProgramResult {
    let it = &mut accounts.iter();
    let admin = next_account_info(it)?;
    let season_ai = next_account_info(it)?;
    let vault = next_account_info(it)?;
    let mint = next_account_info(it)?;
    let token_program = next_account_info(it)?;
    let _system = next_account_info(it)?;
    signer(admin)?;
    let rules = rules_for(preset, market)?;
    let max = (rules.max_civs as usize).min(MAX_NATIONS).min(permutation_rules::genesis::NATIONS.len());
    if !(2..=max as u8).contains(&nations) || tick_seconds == 0 {
        return Err(ChainError::InvalidParams.into());
    }
    if token_program.key != &token::TOKEN_PROGRAM_ID {
        return Err(ProgramError::IncorrectProgramId);
    }
    let decimals = token::mint_decimals(mint).ok_or(ChainError::WrongMint)?;
    let id = season_id.to_le_bytes();
    let bump = expect_pda(program_id, season_ai, &season_seeds(&id))?;
    let vault_bump = expect_pda(program_id, vault, &[VAULT_SEED, &id])?;
    init_pda(admin, season_ai, SEASON_SPACE, program_id, &[SEASON_SEED, &id, &[bump]])?;
    init_pda(admin, vault, token::TOKEN_ACCOUNT_LEN, &token::TOKEN_PROGRAM_ID, &[VAULT_SEED, &id, &[vault_bump]])?;
    token::initialize_account3(vault, mint, season_ai.key)?;
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
    };
    store(&mut season_ai.try_borrow_mut_data()?, &season)?;
    msg!("PS season {} created: preset {} nations {} fee {} market {}", season_id, preset, nations, entry_fee, market);
    Ok(())
}

fn alloc_world(program_id: &Pubkey, accounts: &[AccountInfo], chunk: u8) -> ProgramResult {
    let it = &mut accounts.iter();
    let payer = next_account_info(it)?;
    let season_ai = next_account_info(it)?;
    let world = next_account_info(it)?;
    let _system = next_account_info(it)?;
    signer(payer)?;
    let season = load_season(program_id, season_ai)?;
    if chunk as usize >= WORLD_CHUNKS {
        return Err(ChainError::InvalidParams.into());
    }
    let id = season.season_id.to_le_bytes();
    let bump = expect_pda(program_id, world, &[WORLD_SEED, &id, &[chunk]])?;
    init_pda(payer, world, CHUNK, program_id, &[WORLD_SEED, &id, &[chunk], &[bump]])
}

fn alloc_nation(program_id: &Pubkey, accounts: &[AccountInfo], civ: u16) -> ProgramResult {
    let it = &mut accounts.iter();
    let payer = next_account_info(it)?;
    let season_ai = next_account_info(it)?;
    let nation_ai = next_account_info(it)?;
    let _system = next_account_info(it)?;
    signer(payer)?;
    let season = load_season(program_id, season_ai)?;
    if civ >= season.nations as u16 {
        return Err(ChainError::InvalidParams.into());
    }
    let id = season.season_id.to_le_bytes();
    let civ_bytes = civ.to_le_bytes();
    let bump = expect_pda(program_id, nation_ai, &[NATION_SEED, &id, &civ_bytes])?;
    init_pda(payer, nation_ai, NATION_SPACE, program_id, &[NATION_SEED, &id, &civ_bytes, &[bump]])?;
    let n = NationAccount {
        magic: NATION_MAGIC,
        season_id: season.season_id,
        civ,
        bump,
        preset: season.preset,
        market: season.market,
        crank: season.crank,
        open_tick: u16::MAX, // opened with the government
        officers: [NOBODY; 4],
        keys: [[0; 32]; 4],
        spendable: [0; 4],
        submitted: [u16::MAX; 4],
        frozen: false,
        batches: [None, None, None, None],
        inbox: Vec::new(),
    };
    store(&mut nation_ai.try_borrow_mut_data()?, &n)
}

#[allow(clippy::too_many_arguments)]
fn register(
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
) -> ProgramResult {
    let it = &mut accounts.iter();
    let wallet = next_account_info(it)?;
    let fee_payer = next_account_info(it)?;
    let season_ai = next_account_info(it)?;
    let member_ai = next_account_info(it)?;
    let wallet_token = next_account_info(it)?;
    let vault = next_account_info(it)?;
    let mint = next_account_info(it)?;
    let token_program = next_account_info(it)?;
    let _system = next_account_info(it)?;
    signer(wallet)?;
    signer(fee_payer)?;
    let mut season = load_season(program_id, season_ai)?;
    if season.status != SeasonStatus::Registering {
        return Err(ChainError::WrongStatus.into());
    }
    if civ >= season.nations as u16 {
        return Err(ChainError::InvalidParams.into());
    }
    if season.member_count >= MAX_MEMBERS {
        return Err(ChainError::SeasonFull.into());
    }
    if name.is_empty() || name.len() > MAX_NAME || kind > 2 || stand > 0x0f {
        return Err(ChainError::InvalidName.into());
    }
    if deposit > 0 && !season.market {
        return Err(ChainError::InvalidParams.into());
    }
    if token_program.key != &token::TOKEN_PROGRAM_ID || mint.key.as_ref() != season.usdc_mint {
        return Err(ChainError::WrongMint.into());
    }
    let id = season.season_id.to_le_bytes();
    expect_pda(program_id, vault, &[VAULT_SEED, &id])?;
    let (from_mint, _) = token::token_account_mint_owner(wallet_token).ok_or(ChainError::WrongTokenAccount)?;
    if from_mint != *mint.key {
        return Err(ChainError::WrongTokenAccount.into());
    }
    // One member per wallet per season (V5 D1): the PDA is keyed by the wallet.
    let bump = expect_pda(program_id, member_ai, &[MEMBER_SEED, &id, wallet.key.as_ref()])?;
    init_pda(fee_payer, member_ai, MEMBER_SPACE, program_id, &[MEMBER_SEED, &id, wallet.key.as_ref(), &[bump]])?;
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
    };
    store(&mut member_ai.try_borrow_mut_data()?, &member)?;
    let total = season.entry_fee.checked_add(deposit).ok_or(ChainError::InvalidParams)?;
    if total > 0 {
        token::transfer_checked(wallet_token, mint, vault, wallet, total, season.usdc_decimals, None)?;
    }
    // Entry fees: 80% prize pool, 20% operations (V5 D10).
    let rules = rules_for(season.preset, season.market)?;
    let ops = season.entry_fee * rules.ops_share_bps as u64 / 10_000;
    season.ops += ops;
    season.pool += season.entry_fee - ops;
    season.treasury[civ as usize] += deposit;
    season.nation_members[civ as usize] += 1;
    season.member_count += 1;
    store(&mut season_ai.try_borrow_mut_data()?, &season)?;
    msg!("PS member {} joined nation {} season {} pool {}", member.index, civ, season.season_id, season.pool);
    Ok(())
}

fn update_member(program_id: &Pubkey, accounts: &[AccountInfo], stand: u8, votes: [u32; 4]) -> ProgramResult {
    let it = &mut accounts.iter();
    let who = next_account_info(it)?;
    let season_ai = next_account_info(it)?;
    let member_ai = next_account_info(it)?;
    signer(who)?;
    let season = load_season(program_id, season_ai)?;
    if season.status != SeasonStatus::Registering {
        return Err(ChainError::WrongStatus.into());
    }
    let mut m = load_member(program_id, member_ai, season.season_id)?;
    if who.key.as_ref() != m.wallet && who.key.as_ref() != m.session {
        return Err(ChainError::Unauthorized.into());
    }
    if stand > 0x0f {
        return Err(ChainError::InvalidParams.into());
    }
    m.stand = stand;
    m.votes = votes;
    store(&mut member_ai.try_borrow_mut_data()?, &m)
}

fn start_season(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let it = &mut accounts.iter();
    let authority = next_account_info(it)?;
    let season_ai = next_account_info(it)?;
    let hashes = next_account_info(it)?;
    signer(authority)?;
    let mut season = load_season(program_id, season_ai)?;
    if !is_operator(&season, authority.key) {
        return Err(ChainError::Unauthorized.into());
    }
    if season.status != SeasonStatus::Registering || season.member_count == 0 {
        return Err(ChainError::WrongStatus.into());
    }
    let world = world_chunks(program_id, &accounts[3..], season.season_id)?;
    if hashes.key != &slot_hashes::id() {
        return Err(ChainError::InvalidParams.into());
    }
    // Season seed (§0.2): the latest slot hash, mixed with the season id and
    // the registration totals, taken when registration closes. Documented
    // limitation: the slot leader could grind it; MagicBlock VRF is the plan.
    let recent = hashes.try_borrow_data()?.get(16..48).map(|h| h.to_vec()).ok_or(ChainError::InvalidParams)?;
    let id = season.season_id.to_le_bytes();
    let count = season.member_count.to_le_bytes();
    let treasury = borsh::to_vec(&season.treasury).map_err(|_| ChainError::InvalidParams)?;
    season.season_seed = hashv(&[b"PS/season-seed/v5", &recent, &id, &count, &treasury]).to_bytes();
    let rules = rules_for(season.preset, season.market)?;
    let mut entries = nation_entries(season.nations as usize);
    for (e, t) in entries.iter_mut().zip(&season.treasury) {
        e.treasury = *t;
    }
    check_entries(&rules, &entries).map_err(|_| ChainError::Rules)?;
    let map = MapJob::new(&rules, entries.len()).map_err(|_| ChainError::Rules)?;
    let job = GenesisJob { world_seed: season.world_seed, season_seed: season.season_seed, entries, map };
    world.write_genesis(&job)?;
    let meta = WorldMeta { season_id: season.season_id, preset: season.preset, civs: season.nations, tick_seconds: season.tick_seconds, deadline: 0, finished: false, market: season.market, ..Default::default() };
    world.set_meta(&meta)?;
    season.status = SeasonStatus::Genesis;
    store(&mut season_ai.try_borrow_mut_data()?, &season)?;
    msg!("PS season {} genesis started: {} nations, {} members", season.season_id, season.nations, season.member_count);
    Ok(())
}

fn genesis_step(program_id: &Pubkey, accounts: &[AccountInfo], work: u32) -> ProgramResult {
    let season_ai = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let mut season = load_season(program_id, season_ai)?;
    if season.status != SeasonStatus::Genesis {
        return Err(ChainError::WrongStatus.into());
    }
    let world = world_chunks(program_id, &accounts[1..], season.season_id)?;
    let rules = rules_for(season.preset, season.market)?;
    let GenesisJob { world_seed, season_seed, entries, map } = world.read_genesis()?;
    match map.step(&rules, &world_seed, work).map_err(|_| ChainError::Rules)? {
        MapStep::Working(map) => {
            msg!("PS genesis attempt {} scanned {} rounds {}", map.attempt, map.scanned, map.rounds);
            world.write_genesis(&GenesisJob { world_seed, season_seed, entries, map })
        }
        MapStep::Done(generated) => {
            let state = season_from_map(&rules, &world_seed, &season_seed, &entries, generated).map_err(|_| ChainError::Rules)?;
            let root = world.write_world(&state)?;
            season.status = SeasonStatus::Seating;
            store(&mut season_ai.try_borrow_mut_data()?, &season)?;
            solana_program::log::sol_log_data(&[b"PS_GENESIS", &root, &season_seed]);
            msg!("PS genesis complete");
            Ok(())
        }
    }
}

/// Add the next members (registration order) to the world with their
/// pre-season candidacy and votes. Logged as `PS_SEAT ‖ borsh(members)` for
/// the verifier.
fn seat_members(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let authority = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let season_ai = accounts.get(1).ok_or(ProgramError::NotEnoughAccountKeys)?;
    signer(authority)?;
    let mut season = load_season(program_id, season_ai)?;
    if !is_operator(&season, authority.key) {
        return Err(ChainError::Unauthorized.into());
    }
    if season.status != SeasonStatus::Seating {
        return Err(ChainError::WrongStatus.into());
    }
    let world = world_chunks(program_id, &accounts[2..], season.season_id)?;
    let rules = rules_for(season.preset, season.market)?;
    let mut state = world.read_world()?;
    let mut seated = Vec::new();
    for ai in &accounts[2 + WORLD_CHUNKS..] {
        let m = load_member(program_id, ai, season.season_id)?;
        if m.index != state.members.len() as u32 {
            return Err(ChainError::InvalidParams.into());
        }
        let id = gov::join(&mut state, &rules, m.civ, m.session).map_err(|_| ChainError::Rules)?;
        let mut entries = vec![GovEntry { member: id, signer: m.session, action: GovAction::Stand { roles: m.stand } }];
        for role in Role::ALL {
            let candidate = m.votes[role.index()];
            if candidate != NOBODY {
                entries.push(GovEntry { member: id, signer: m.session, action: GovAction::Vote { role, candidate } });
            }
        }
        gov::apply_pre_season(&mut state, &rules, &entries).map_err(|_| ChainError::Rules)?;
        seated.push((m.civ, m.session, m.stand, m.votes));
    }
    season.seated = state.members.len() as u32;
    let root = world.write_world(&state)?;
    store(&mut season_ai.try_borrow_mut_data()?, &season)?;
    let record = borsh::to_vec(&seated).map_err(|_| ChainError::Rules)?;
    solana_program::log::sol_log_data(&[b"PS_SEAT", &root, &record]);
    msg!("PS seated {} members ({} of {})", seated.len(), season.seated, season.member_count);
    Ok(())
}

fn open_government(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let authority = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let season_ai = accounts.get(1).ok_or(ProgramError::NotEnoughAccountKeys)?;
    signer(authority)?;
    let mut season = load_season(program_id, season_ai)?;
    if !is_operator(&season, authority.key) {
        return Err(ChainError::Unauthorized.into());
    }
    if season.status != SeasonStatus::Seating || season.seated != season.member_count {
        return Err(ChainError::WrongStatus.into());
    }
    let world = world_chunks(program_id, &accounts[2..], season.season_id)?;
    let rules = rules_for(season.preset, season.market)?;
    let mut state = world.read_world()?;
    gov::first_election(&mut state, &rules).map_err(|_| ChainError::Rules)?;
    let it = &mut accounts[2 + WORLD_CHUNKS..].iter();
    for civ in 0..season.nations as u16 {
        let ai = next_account_info(it)?;
        let mut n = load_nation(program_id, ai, season.season_id, civ)?;
        open_nation(&mut n, &state, &rules);
        store(&mut ai.try_borrow_mut_data()?, &n)?;
    }
    let root = world.write_world(&state)?;
    let mut meta = world.meta()?;
    meta.deadline = Clock::get()?.unix_timestamp + meta.tick_seconds as i64;
    world.set_meta(&meta)?;
    season.status = SeasonStatus::Running;
    store(&mut season_ai.try_borrow_mut_data()?, &season)?;
    solana_program::log::sol_log_data(&[b"PS_OPEN", &root]);
    msg!("PS government opened: tick 0 is open");
    Ok(())
}

// ------------------------------------------------------------------ base: ER lifecycle

fn delegate(program_id: &Pubkey, accounts: &[AccountInfo], target: u16) -> ProgramResult {
    let it = &mut accounts.iter();
    let authority = next_account_info(it)?;
    let system = next_account_info(it)?;
    let season_ai = next_account_info(it)?;
    let pda = next_account_info(it)?;
    let owner_program = next_account_info(it)?;
    let buffer = next_account_info(it)?;
    let record = next_account_info(it)?;
    let metadata = next_account_info(it)?;
    let delegation_program = next_account_info(it)?;
    let validator = it.next();
    signer(authority)?;
    let season = load_season(program_id, season_ai)?;
    if !is_operator(&season, authority.key) {
        return Err(ChainError::Unauthorized.into());
    }
    if season.status != SeasonStatus::Running {
        return Err(ChainError::WrongStatus.into());
    }
    if owner_program.key != program_id || system.key != &system_program::id() {
        return Err(ProgramError::IncorrectProgramId);
    }
    if delegation_program.key != &DELEGATION_PROGRAM_ID {
        return Err(ChainError::WrongDelegationProgram.into());
    }
    let id = season.season_id.to_le_bytes();
    let civ_bytes;
    let chunk;
    let seeds: Vec<&[u8]> = if (target as usize) < WORLD_CHUNKS {
        chunk = [target as u8];
        vec![WORLD_SEED, &id, &chunk]
    } else if target >= NATION_TARGET && target - NATION_TARGET < season.nations as u16 {
        civ_bytes = (target - NATION_TARGET).to_le_bytes();
        vec![NATION_SEED, &id, &civ_bytes]
    } else {
        return Err(ChainError::InvalidParams.into());
    };
    expect_pda(program_id, pda, &seeds)?;
    delegate_account(
        DelegateAccounts {
            payer: authority,
            pda,
            owner_program,
            buffer,
            delegation_record: record,
            delegation_metadata: metadata,
            delegation_program,
            system_program: system,
        },
        &seeds,
        DelegateConfig { commit_frequency_ms: ER_COMMIT_FREQUENCY_MS, validator: validator.map(|v| *v.key) },
    )?;
    msg!("PS delegated {} target {}", pda.key, target);
    Ok(())
}

fn undelegate_callback(program_id: &Pubkey, accounts: &[AccountInfo], seeds: Vec<Vec<u8>>) -> ProgramResult {
    let it = &mut accounts.iter();
    let pda = next_account_info(it)?;
    let buffer = next_account_info(it)?;
    let payer = next_account_info(it)?;
    let system = next_account_info(it)?;
    if buffer.owner != &DELEGATION_PROGRAM_ID {
        return Err(ChainError::WrongDelegationProgram.into());
    }
    // Only our own PDAs, with the content they are supposed to hold.
    let refs: Vec<&[u8]> = seeds.iter().map(Vec::as_slice).collect();
    expect_pda(program_id, pda, &refs)?;
    let data = buffer.try_borrow_data()?;
    let ok = match refs.first().copied() {
        // Chunk 0 carries the header; the others are raw continuation bytes.
        Some(s) if s == WORLD_SEED => {
            data.len() == CHUNK && (refs.get(2) != Some(&&[0u8][..]) || data[..8] == WORLD_MAGIC || data[..8] == GENESIS_MAGIC)
        }
        Some(s) if s == NATION_SEED => data.len() >= 8 && data[..8] == NATION_MAGIC,
        _ => false,
    };
    drop(data);
    if !ok {
        return Err(ChainError::WrongPda.into());
    }
    undelegate_account(pda, program_id, buffer, payer, system, seeds)?;
    msg!("PS undelegated {}", pda.key);
    Ok(())
}

fn commit(program_id: &Pubkey, accounts: &[AccountInfo], undelegate: bool) -> ProgramResult {
    let it = &mut accounts.iter();
    let payer = next_account_info(it)?;
    let magic_program = next_account_info(it)?;
    let magic_context = next_account_info(it)?;
    signer(payer)?;
    if magic_program.key != &MAGIC_PROGRAM_ID || magic_context.key != &MAGIC_CONTEXT_ID {
        return Err(ChainError::WrongMagicProgram.into());
    }
    let chunk0 = accounts.get(3).ok_or(ProgramError::NotEnoughAccountKeys)?;
    let world = world_chunks(program_id, &accounts[3..], world_season_id(chunk0)?)?;
    let meta = world.meta()?;
    if undelegate && !meta.finished {
        return Err(ChainError::SeasonNotOver.into());
    }
    let it = &mut accounts[3 + WORLD_CHUNKS..].iter();
    let mut list: Vec<AccountInfo> = world.accounts.to_vec();
    for civ in 0..meta.civs as u16 {
        let ai = next_account_info(it)?;
        load_nation(program_id, ai, meta.season_id, civ)?;
        list.push(ai.clone());
    }
    let builder = MagicIntentBundleBuilder::new(payer.clone(), magic_context.clone(), magic_program.clone());
    if undelegate {
        builder.commit_and_undelegate(&list).build_and_invoke()?;
    } else {
        builder.commit(&list).build_and_invoke()?;
    }
    msg!("PS commit{} season {}", if undelegate { "+undelegate" } else { "" }, meta.season_id);
    Ok(())
}

fn undelegate_part(program_id: &Pubkey, accounts: &[AccountInfo], targets: Vec<u16>) -> ProgramResult {
    let it = &mut accounts.iter();
    let payer = next_account_info(it)?;
    let magic_program = next_account_info(it)?;
    let magic_context = next_account_info(it)?;
    let chunk0 = next_account_info(it)?;
    signer(payer)?;
    if magic_program.key != &MAGIC_PROGRAM_ID || magic_context.key != &MAGIC_CONTEXT_ID {
        return Err(ChainError::WrongMagicProgram.into());
    }
    if targets.is_empty() || targets.len() > WORLD_CHUNKS + MAX_NATIONS {
        return Err(ChainError::InvalidParams.into());
    }
    // The season is over: read from chunk 0's header.
    let season_id = world_season_id(chunk0)?;
    let id = season_id.to_le_bytes();
    if chunk0.owner != program_id || chunk0.data_len() != CHUNK {
        return Err(ChainError::WrongWorld.into());
    }
    expect_pda(program_id, chunk0, &[WORLD_SEED, &id, &[0]])?;
    let meta = WorldMeta::deserialize(&mut &chunk0.try_borrow_data()?[12..WORLD_HEADER]).map_err(|_| ChainError::WrongWorld)?;
    if !meta.finished {
        return Err(ChainError::SeasonNotOver.into());
    }
    let mut list: Vec<AccountInfo> = Vec::with_capacity(targets.len());
    for (i, &t) in targets.iter().enumerate() {
        if targets[..i].contains(&t) {
            return Err(ChainError::InvalidParams.into());
        }
        if t == 0 {
            list.push(chunk0.clone());
            continue;
        }
        let ai = next_account_info(it)?;
        if (t as usize) < WORLD_CHUNKS {
            if ai.owner != program_id {
                return Err(ChainError::WrongWorld.into());
            }
            expect_pda(program_id, ai, &[WORLD_SEED, &id, &[t as u8]])?;
        } else {
            let civ = t.checked_sub(NATION_TARGET).filter(|c| *c < meta.civs as u16).ok_or(ChainError::InvalidParams)?;
            load_nation(program_id, ai, season_id, civ)?;
        }
        list.push(ai.clone());
    }
    MagicIntentBundleBuilder::new(payer.clone(), magic_context.clone(), magic_program.clone()).commit_and_undelegate(&list).build_and_invoke()?;
    msg!("PS undelegate {:?} season {}", targets, season_id);
    Ok(())
}

// ------------------------------------------------------------------ ER: play

#[allow(clippy::too_many_arguments)]
fn submit_orders(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    role: Role,
    tick: u16,
    decision_digest: [u8; 32],
    orders: Vec<Order>,
    adopt: Vec<u32>,
) -> ProgramResult {
    let it = &mut accounts.iter();
    let who = next_account_info(it)?;
    let nation_ai = next_account_info(it)?;
    signer(who)?;
    if nation_ai.owner != program_id {
        return Err(ChainError::MissingNation.into());
    }
    let mut n: NationAccount = load(&nation_ai.try_borrow_data()?)?;
    if n.magic != NATION_MAGIC {
        return Err(ChainError::MissingNation.into());
    }
    expect_pda(program_id, nation_ai, &[NATION_SEED, &n.season_id.to_le_bytes(), &n.civ.to_le_bytes()])?;
    let i = role.index();
    let member = n.officers[i];
    // The office holder's session key; the crank for a vacant office (the acting official).
    let allowed = if member == NOBODY { n.crank } else { n.keys[i] };
    if who.key.as_ref() != allowed {
        return Err(ChainError::Unauthorized.into());
    }
    if tick != n.open_tick {
        return Err(ChainError::WrongTick.into());
    }
    if n.frozen {
        return Err(ChainError::TickFrozen.into());
    }
    // Officers seal a rationale (V5 D17).
    if member != NOBODY && decision_digest == [0; 32] {
        return Err(ChainError::Rules.into());
    }
    if !orders.iter().all(|o| role_allows_static(role, o)) {
        return Err(ChainError::WrongOffice.into());
    }
    let rules = rules_for(n.preset, n.market)?;
    let cost = check_structure(&rules, tick, &orders).map_err(|_| ChainError::Rules)?;
    if cost > n.spendable[i] || adopt.len() > rules.max_open_proposals as usize {
        return Err(ChainError::OverBudget.into());
    }
    n.batches[i] = Some(OrderBatch { civ: n.civ, tick, role, member, decision_digest, orders, adopt });
    n.submitted[i] = tick;
    store(&mut nation_ai.try_borrow_mut_data()?, &n)?;
    Ok(())
}

fn submit_gov(program_id: &Pubkey, accounts: &[AccountInfo], member: u32, action: GovAction) -> ProgramResult {
    let it = &mut accounts.iter();
    let who = next_account_info(it)?;
    let nation_ai = next_account_info(it)?;
    signer(who)?;
    if nation_ai.owner != program_id {
        return Err(ChainError::MissingNation.into());
    }
    let mut n: NationAccount = load(&nation_ai.try_borrow_data()?)?;
    if n.magic != NATION_MAGIC {
        return Err(ChainError::MissingNation.into());
    }
    expect_pda(program_id, nation_ai, &[NATION_SEED, &n.season_id.to_le_bytes(), &n.civ.to_le_bytes()])?;
    if n.open_tick == u16::MAX {
        return Err(ChainError::WrongStatus.into());
    }
    if n.frozen {
        return Err(ChainError::TickFrozen.into());
    }
    let signer_key = who.key.to_bytes();
    if n.inbox.iter().filter(|e| e.signer == signer_key).count() >= MAX_GOV_PER_SIGNER {
        return Err(ChainError::InboxFull.into());
    }
    n.inbox.push(GovEntry { member, signer: signer_key, action });
    // Fails with AccountDataTooSmall when the inbox is full for this tick.
    store(&mut nation_ai.try_borrow_mut_data()?, &n)
}

/// The open tick's input as it stands in the nation accounts (batches for
/// the open tick, governance inboxes), with `vrf` as its randomness.
fn pending_input<'a, 'info>(
    program_id: &Pubkey,
    nations: &'a [AccountInfo<'info>],
    meta: &WorldMeta,
    vrf: [u8; 32],
) -> Result<(TickInput, u16, usize, Vec<&'a AccountInfo<'info>>), ProgramError> {
    let it = &mut nations.iter();
    let mut ais = Vec::with_capacity(meta.civs as usize);
    let (mut batches, mut gov, mut done, mut tick) = (Vec::new(), Vec::new(), 0, u16::MAX);
    for civ in 0..meta.civs as u16 {
        let ai = next_account_info(it)?;
        let n = load_nation(program_id, ai, meta.season_id, civ)?;
        if tick == u16::MAX {
            tick = n.open_tick;
        }
        for b in n.batches.into_iter().flatten().filter(|b| b.tick == tick) {
            batches.push(b);
            done += 1;
        }
        gov.extend(n.inbox);
        ais.push(ai);
    }
    Ok((TickInput { vrf, batches, gov, deposits: Vec::new() }, tick, done, ais))
}

fn log_tick_input(program_id: &Pubkey, accounts: &[AccountInfo], chunk: u16) -> ProgramResult {
    let chunk0 = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let world = world_chunks(program_id, accounts, world_season_id(chunk0)?)?;
    let mut meta = world.meta()?;
    if meta.finished {
        return Err(ChainError::WrongStatus.into());
    }
    let nations = &accounts[WORLD_CHUNKS..];
    if !meta.frozen {
        if chunk != 0 {
            return Err(ChainError::InputNotPublished.into());
        }
        let (_, _, done, ais) = pending_input(program_id, nations, &meta, [0; 32])?;
        let clock = Clock::get()?;
        if clock.unix_timestamp < meta.deadline && done < 4 * meta.civs as usize {
            return Err(ChainError::TooEarly.into());
        }
        // Tick randomness (§0.2): the committed world plus this slot, fixed
        // before anyone can see the input's effect. MagicBlock VRF is the
        // planned replacement.
        let body = world.body(&WORLD_MAGIC)?;
        let root = hashv(&[&body]).to_bytes();
        drop(body);
        meta.vrf = hashv(&[b"PS/tick-vrf/v2", &root, &clock.slot.to_le_bytes(), &clock.unix_timestamp.to_le_bytes()]).to_bytes();
        meta.frozen = true;
        meta.input_logged = 0;
        for ai in ais {
            let mut n: NationAccount = load(&ai.try_borrow_data()?)?;
            n.frozen = true;
            store(&mut ai.try_borrow_mut_data()?, &n)?;
        }
    }
    let (input, tick, _, _) = pending_input(program_id, nations, &meta, meta.vrf)?;
    let bytes = borsh::to_vec(&input).map_err(|_| ChainError::Rules)?;
    let total = bytes.len().div_ceil(INPUT_CHUNK).max(1) as u16;
    if chunk >= total || chunk > meta.input_logged {
        return Err(ChainError::InvalidParams.into());
    }
    meta.input_chunks = total;
    meta.input_logged = meta.input_logged.max(chunk + 1);
    world.set_meta(&meta)?;
    let hash = hashv(&[&bytes]).to_bytes();
    let part = &bytes[chunk as usize * INPUT_CHUNK..bytes.len().min((chunk as usize + 1) * INPUT_CHUNK)];
    solana_program::log::sol_log_data(&[b"PS_INPUT", &tick.to_le_bytes(), &chunk.to_le_bytes(), &total.to_le_bytes(), &hash, part]);
    Ok(())
}

fn resolve_tick(program_id: &Pubkey, accounts: &[AccountInfo], to: u8) -> ProgramResult {
    let chunk0 = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let world = world_chunks(program_id, accounts, world_season_id(chunk0)?)?;
    let mut meta = world.meta()?;
    if meta.finished {
        return Err(ChainError::WrongStatus.into());
    }
    // Data availability: a tick resolves only on an input the chain published.
    if !meta.frozen || meta.input_logged < meta.input_chunks || meta.input_chunks == 0 {
        return Err(ChainError::InputNotPublished.into());
    }
    let body = world.body(&WORLD_MAGIC)?;
    let pre_root = hashv(&[&body]).to_bytes();
    let mut state = WorldState::try_from_slice(&body).map_err(|_| ChainError::WrongWorld)?;
    drop(body);
    let rules = rules_for(meta.preset, meta.market)?;
    // Submissions are refused while frozen, so this is the input PS_INPUT published.
    let (input, _, _, nation_ais) = pending_input(program_id, &accounts[WORLD_CHUNKS..], &meta, meta.vrf)?;
    let tick = state.tick;
    while state.phase_cursor < to.min(12) {
        let p = state.phase_cursor;
        run_phase(&mut state, &rules, &input, p).map_err(|_| ChainError::Rules)?;
        if state.phase_cursor == 0 {
            break; // the tick completed
        }
    }
    let completed = state.tick != tick;
    if completed {
        for ai in &nation_ais {
            let mut n: NationAccount = load(&ai.try_borrow_data()?)?;
            open_nation(&mut n, &state, &rules);
            store(&mut ai.try_borrow_mut_data()?, &n)?;
        }
        let clock = Clock::get()?;
        meta.deadline = clock.unix_timestamp + meta.tick_seconds as i64;
        meta.finished = state.tick >= rules.ticks_per_season;
        (meta.frozen, meta.vrf, meta.input_chunks, meta.input_logged) = (false, [0; 32], 0, 0);
    }
    let root = world.write_world(&state)?;
    world.set_meta(&meta)?;
    // Tick record for the replay verifier: the step from the previous root
    // to this one, and the hash of the input (published in full as PS_INPUT
    // before the tick could resolve). A split tick logs one record per part.
    let record = borsh::to_vec(&input).map_err(|_| ChainError::Rules)?;
    let input_hash = hashv(&[&record]).to_bytes();
    solana_program::log::sol_log_data(&[b"PS_TICK", &tick.to_le_bytes(), &[to], &pre_root, &root, &input_hash]);
    msg!("PS tick {} {}", tick, if completed { "resolved" } else { "partial" });
    Ok(())
}

// ------------------------------------------------------------------ base: settlement

fn finish_season(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
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
    season.payouts = s.per_member;
    season.treasury_final = state.civs.iter().map(|c| c.usdc).collect();
    season.final_root = state.state_root().map_err(|_| ChainError::Rules)?;
    season.status = SeasonStatus::Finalized;
    store(&mut season_ai.try_borrow_mut_data()?, &season)?;
    msg!("PS season {} finalized: pool {} ops {} refund {}", season.season_id, season.pool, season.ops, s.refund);
    Ok(())
}

/// What a member receives: its prize, plus its share of what is left in
/// its nation's treasury (V5 §7.5).
pub fn claim_amount(season: &Season, member: &MemberAccount) -> u64 {
    let prize = season.payouts.get(member.index as usize).copied().unwrap_or(0);
    let civ = member.civ as usize;
    let deposited = season.treasury.get(civ).copied().unwrap_or(0);
    let left = season.treasury_final.get(civ).copied().unwrap_or(0);
    let refund = if deposited == 0 { 0 } else { (member.shares as u128 * left as u128 / deposited as u128) as u64 };
    prize + refund
}

fn claim(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
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
    if token_program.key != &token::TOKEN_PROGRAM_ID || mint.key.as_ref() != season.usdc_mint {
        return Err(ChainError::WrongMint.into());
    }
    let id = season.season_id.to_le_bytes();
    expect_pda(program_id, vault, &[VAULT_SEED, &id])?;
    let (dmint, downer) = token::token_account_mint_owner(dest).ok_or(ChainError::WrongTokenAccount)?;
    if dmint != *mint.key || downer != *wallet.key {
        return Err(ChainError::WrongTokenAccount.into());
    }
    member.claimed = true;
    store(&mut member_ai.try_borrow_mut_data()?, &member)?;
    token::transfer_checked(vault, mint, dest, season_ai, amount, season.usdc_decimals, Some(&[SEASON_SEED, &id, &[season.bump]]))?;
    msg!("PS claim season {} member {} amount {}", season.season_id, member.index, amount);
    Ok(())
}

fn withdraw_ops(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
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
    if token_program.key != &token::TOKEN_PROGRAM_ID || mint.key.as_ref() != season.usdc_mint {
        return Err(ChainError::WrongMint.into());
    }
    let id = season.season_id.to_le_bytes();
    expect_pda(program_id, vault, &[VAULT_SEED, &id])?;
    let (dmint, _) = token::token_account_mint_owner(dest).ok_or(ChainError::WrongTokenAccount)?;
    if dmint != *mint.key {
        return Err(ChainError::WrongTokenAccount.into());
    }
    season.ops_withdrawn = true;
    let amount = season.ops;
    store(&mut season_ai.try_borrow_mut_data()?, &season)?;
    if amount > 0 {
        token::transfer_checked(vault, mint, dest, season_ai, amount, season.usdc_decimals, Some(&[SEASON_SEED, &id, &[season.bump]]))?;
    }
    msg!("PS operations share {} withdrawn", amount);
    Ok(())
}
