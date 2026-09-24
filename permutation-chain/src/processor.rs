use borsh::BorshDeserialize;
use ephemeral_rollups_sdk::{
    consts::{DELEGATION_PROGRAM_ID, MAGIC_CONTEXT_ID, MAGIC_PROGRAM_ID},
    cpi::{delegate_account, undelegate_account, DelegateAccounts, DelegateConfig},
    ephem::{FoldableIntentBuilder, MagicIntentBundleBuilder},
};
use permutation_rules::genesis::{check_entries, season_from_map, Entry};
use permutation_rules::map::{MapJob, MapStep};
use permutation_rules::orders::{check_structure, OrderBatch};
use permutation_rules::state::DeclaredKind;
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
/// `Delegate { target }`: world chunks are 0..WORLD_CHUNKS, orders are ORDERS_TARGET + civ.
pub const ORDERS_TARGET: u16 = 1000;
/// How often the ER auto-commits delegated accounts to the base layer.
pub const ER_COMMIT_FREQUENCY_MS: u32 = 30_000;

pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    if let Some(seeds) = data.strip_prefix(&UNDELEGATE_CALLBACK_DISCRIMINATOR) {
        let seeds = Vec::<Vec<u8>>::try_from_slice(seeds).map_err(|_| ChainError::InvalidInstruction)?;
        return undelegate_callback(program_id, accounts, seeds);
    }
    let ix = ChainInstruction::try_from_slice(data).map_err(|_| ChainError::InvalidInstruction)?;
    match ix {
        ChainInstruction::CreateSeason { season_id, preset, max_civs, entry_fee, exchange_credit, tick_seconds, world_seed, crank } => {
            create_season(program_id, accounts, season_id, preset, max_civs, entry_fee, exchange_credit, tick_seconds, world_seed, crank)
        }
        ChainInstruction::AllocWorld { chunk } => alloc_world(program_id, accounts, chunk),
        ChainInstruction::JoinSeason { name, kind, session, payout } => join_season(program_id, accounts, name, kind, session, payout),
        ChainInstruction::StartSeason => start_season(program_id, accounts),
        ChainInstruction::GenesisStep { work } => genesis_step(program_id, accounts, work),
        ChainInstruction::Delegate { target } => delegate(program_id, accounts, target),
        ChainInstruction::SubmitOrders { tick, decision_digest, orders } => submit_orders(program_id, accounts, tick, decision_digest, orders),
        ChainInstruction::ResolveTick { to } => resolve_tick(program_id, accounts, to),
        ChainInstruction::Commit => commit(program_id, accounts, false),
        ChainInstruction::CommitAndUndelegate => commit(program_id, accounts, true),
        ChainInstruction::FinishSeason => finish_season(program_id, accounts),
        ChainInstruction::Claim { civ } => claim(program_id, accounts, civ),
        ChainInstruction::UndelegatePart { targets } => undelegate_part(program_id, accounts, targets),
    }
}

// ------------------------------------------------------------------ helpers

fn rules_for(preset: u8) -> Result<Ruleset, ProgramError> {
    match preset {
        0 => Ok(Ruleset::new(Preset::Blitz)),
        1 => Ok(Ruleset::new(Preset::Season)),
        _ => Err(ChainError::InvalidParams.into()),
    }
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

fn load_orders(program_id: &Pubkey, account: &AccountInfo, season_id: u64, civ: u16) -> Result<Orders, ProgramError> {
    if account.owner != program_id {
        return Err(ChainError::MissingOrders.into());
    }
    let o: Orders = load(&account.try_borrow_data()?)?;
    if o.magic != ORDERS_MAGIC || o.season_id != season_id || o.civ != civ {
        return Err(ChainError::MissingOrders.into());
    }
    Ok(o)
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
    max_civs: u8,
    entry_fee: u64,
    exchange_credit: u64,
    tick_seconds: u32,
    world_seed: [u8; 32],
    crank: [u8; 32],
) -> ProgramResult {
    let it = &mut accounts.iter();
    let admin = next_account_info(it)?;
    let season_ai = next_account_info(it)?;
    let vault = next_account_info(it)?;
    let mint = next_account_info(it)?;
    let token_program = next_account_info(it)?;
    let _system = next_account_info(it)?;
    signer(admin)?;
    let rules = rules_for(preset)?;
    if !(2..=MAX_CIVS as u8).contains(&max_civs) || max_civs > rules.max_civs || tick_seconds == 0 {
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
        max_civs,
        entry_fee,
        exchange_credit,
        tick_seconds,
        status: SeasonStatus::Registering,
        world_seed,
        season_seed: [0; 32],
        civs: Vec::new(),
        pool: 0,
        payouts: Vec::new(),
        claimed: Vec::new(),
        rollover: 0,
        final_root: [0; 32],
    };
    store(&mut season_ai.try_borrow_mut_data()?, &season)?;
    msg!("PS season {} created: preset {} max {} fee {}", season_id, preset, max_civs, entry_fee);
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

fn join_season(program_id: &Pubkey, accounts: &[AccountInfo], name: String, kind: u8, session: [u8; 32], payout: [u8; 32]) -> ProgramResult {
    let it = &mut accounts.iter();
    let player = next_account_info(it)?;
    let fee_payer = next_account_info(it)?;
    let season_ai = next_account_info(it)?;
    let orders_ai = next_account_info(it)?;
    let player_token = next_account_info(it)?;
    let vault = next_account_info(it)?;
    let mint = next_account_info(it)?;
    let token_program = next_account_info(it)?;
    let _system = next_account_info(it)?;
    signer(player)?;
    signer(fee_payer)?;
    let mut season = load_season(program_id, season_ai)?;
    if season.status != SeasonStatus::Registering {
        return Err(ChainError::WrongStatus.into());
    }
    if season.civs.len() >= season.max_civs as usize {
        return Err(ChainError::SeasonFull.into());
    }
    if name.is_empty() || name.len() > MAX_NAME || kind > 2 {
        return Err(ChainError::InvalidName.into());
    }
    let rules = rules_for(season.preset)?;
    if season.civs.iter().filter(|c| c.payout == payout).count() >= rules.max_civs_per_wallet as usize {
        return Err(ChainError::TooManyCivs.into());
    }
    if token_program.key != &token::TOKEN_PROGRAM_ID || mint.key.as_ref() != season.usdc_mint {
        return Err(ChainError::WrongMint.into());
    }
    let id = season.season_id.to_le_bytes();
    expect_pda(program_id, vault, &[VAULT_SEED, &id])?;
    let (from_mint, _) = token::token_account_mint_owner(player_token).ok_or(ChainError::WrongTokenAccount)?;
    if from_mint != *mint.key {
        return Err(ChainError::WrongTokenAccount.into());
    }
    let civ = season.civs.len() as u16;
    let civ_bytes = civ.to_le_bytes();
    let bump = expect_pda(program_id, orders_ai, &[ORDERS_SEED, &id, &civ_bytes])?;
    init_pda(fee_payer, orders_ai, ORDERS_SPACE, program_id, &[ORDERS_SEED, &id, &civ_bytes, &[bump]])?;
    let orders = Orders {
        magic: ORDERS_MAGIC,
        season_id: season.season_id,
        civ,
        bump,
        preset: season.preset,
        player: player.key.to_bytes(),
        session,
        open_tick: u16::MAX, // opened when genesis completes
        spendable: 0,
        batch: None,
    };
    store(&mut orders_ai.try_borrow_mut_data()?, &orders)?;
    if season.entry_fee > 0 {
        token::transfer_checked(player_token, mint, vault, player, season.entry_fee, season.usdc_decimals, None)?;
    }
    season.pool += season.entry_fee;
    season.civs.push(CivSlot { player: player.key.to_bytes(), session, payout, kind, name });
    store(&mut season_ai.try_borrow_mut_data()?, &season)?;
    msg!("PS join season {} civ {} pool {}", season.season_id, civ, season.pool);
    Ok(())
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
    if season.status != SeasonStatus::Registering || season.civs.len() < 2 {
        return Err(ChainError::WrongStatus.into());
    }
    let world = world_chunks(program_id, &accounts[3..], season.season_id)?;
    if hashes.key != &slot_hashes::id() {
        return Err(ChainError::InvalidParams.into());
    }
    // Season seed (§0.2): the latest slot hash, mixed with every entrant, so
    // nobody who joined could know it. Documented limitation: the slot leader
    // could grind it; MagicBlock VRF is the planned source.
    let recent = hashes.try_borrow_data()?.get(16..48).map(|h| h.to_vec()).ok_or(ChainError::InvalidParams)?;
    let mut parts: Vec<&[u8]> = vec![b"PS/season-seed/v1", &recent];
    let id = season.season_id.to_le_bytes();
    parts.push(&id);
    for c in &season.civs {
        parts.push(&c.player);
    }
    season.season_seed = hashv(&parts).to_bytes();
    let rules = rules_for(season.preset)?;
    let entries: Vec<Entry> = season
        .civs
        .iter()
        .map(|c| Entry {
            name: c.name.clone(),
            declared_kind: match c.kind {
                0 => DeclaredKind::Human,
                1 => DeclaredKind::Agent,
                _ => DeclaredKind::Undeclared,
            },
            payout_wallet: c.payout,
            exchange_deposit: season.exchange_credit,
        })
        .collect();
    check_entries(&rules, &entries).map_err(|_| ChainError::Rules)?;
    let map = MapJob::new(&rules, entries.len()).map_err(|_| ChainError::Rules)?;
    let job = GenesisJob { world_seed: season.world_seed, season_seed: season.season_seed, entries, map };
    world.write_genesis(&job)?;
    let meta = WorldMeta { season_id: season.season_id, preset: season.preset, civs: season.civs.len() as u8, tick_seconds: season.tick_seconds, deadline: 0, finished: false };
    world.set_meta(&meta)?;
    season.status = SeasonStatus::Genesis;
    store(&mut season_ai.try_borrow_mut_data()?, &season)?;
    msg!("PS season {} genesis started with {} civs", season.season_id, season.civs.len());
    Ok(())
}

fn genesis_step(program_id: &Pubkey, accounts: &[AccountInfo], work: u32) -> ProgramResult {
    let season_ai = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let mut season = load_season(program_id, season_ai)?;
    if season.status != SeasonStatus::Genesis {
        return Err(ChainError::WrongStatus.into());
    }
    let world = world_chunks(program_id, &accounts[1..], season.season_id)?;
    let it = &mut accounts[1 + WORLD_CHUNKS..].iter();
    let rules = rules_for(season.preset)?;
    let GenesisJob { world_seed, season_seed, entries, map } = world.read_genesis()?;
    match map.step(&rules, &world_seed, work).map_err(|_| ChainError::Rules)? {
        MapStep::Working(map) => {
            msg!("PS genesis attempt {} scanned {} rounds {}", map.attempt, map.scanned, map.rounds);
            world.write_genesis(&GenesisJob { world_seed, season_seed, entries, map })
        }
        MapStep::Done(generated) => {
            let state = season_from_map(&rules, &world_seed, &season_seed, &entries, generated).map_err(|_| ChainError::Rules)?;
            // Open tick 0 on every civ's orders account.
            for civ in 0..season.civs.len() as u16 {
                let ai = next_account_info(it)?;
                let mut o = load_orders(program_id, ai, season.season_id, civ)?;
                let c = &state.civs[civ as usize];
                o.open_tick = state.tick;
                o.spendable = c.tick_budget as u32 + c.order_bank as u32;
                o.batch = None;
                store(&mut ai.try_borrow_mut_data()?, &o)?;
            }
            let root = world.write_world(&state)?;
            let mut meta = world.meta()?;
            meta.deadline = Clock::get()?.unix_timestamp + meta.tick_seconds as i64;
            world.set_meta(&meta)?;
            season.status = SeasonStatus::Running;
            store(&mut season_ai.try_borrow_mut_data()?, &season)?;
            solana_program::log::sol_log_data(&[b"PS_GENESIS", &root, &season_seed]);
            msg!("PS genesis complete");
            Ok(())
        }
    }
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
    } else if target >= ORDERS_TARGET {
        civ_bytes = (target - ORDERS_TARGET).to_le_bytes();
        vec![ORDERS_SEED, &id, &civ_bytes]
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
        Some(s) if s == ORDERS_SEED => data.len() >= 8 && data[..8] == ORDERS_MAGIC,
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
        load_orders(program_id, ai, meta.season_id, civ)?;
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
    if targets.is_empty() || targets.len() > WORLD_CHUNKS + MAX_CIVS {
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
            let civ = t.checked_sub(ORDERS_TARGET).filter(|c| *c < meta.civs as u16).ok_or(ChainError::InvalidParams)?;
            load_orders(program_id, ai, season_id, civ)?;
        }
        list.push(ai.clone());
    }
    MagicIntentBundleBuilder::new(payer.clone(), magic_context.clone(), magic_program.clone()).commit_and_undelegate(&list).build_and_invoke()?;
    msg!("PS undelegate {:?} season {}", targets, season_id);
    Ok(())
}

// ------------------------------------------------------------------ ER: play

fn submit_orders(program_id: &Pubkey, accounts: &[AccountInfo], tick: u16, decision_digest: [u8; 32], orders: Vec<permutation_rules::orders::Order>) -> ProgramResult {
    let it = &mut accounts.iter();
    let who = next_account_info(it)?;
    let orders_ai = next_account_info(it)?;
    signer(who)?;
    if orders_ai.owner != program_id {
        return Err(ChainError::MissingOrders.into());
    }
    let mut o: Orders = load(&orders_ai.try_borrow_data()?)?;
    if o.magic != ORDERS_MAGIC {
        return Err(ChainError::MissingOrders.into());
    }
    expect_pda(program_id, orders_ai, &[ORDERS_SEED, &o.season_id.to_le_bytes(), &o.civ.to_le_bytes()])?;
    if who.key.as_ref() != o.session && who.key.as_ref() != o.player {
        return Err(ChainError::Unauthorized.into());
    }
    if tick != o.open_tick {
        return Err(ChainError::WrongTick.into());
    }
    let rules = rules_for(o.preset)?;
    let cost = check_structure(&rules, tick, &orders).map_err(|_| ChainError::Rules)?;
    if cost > o.spendable {
        return Err(ChainError::OverBudget.into());
    }
    o.batch = Some(OrderBatch { civ: o.civ, tick, decision_digest, orders });
    store(&mut orders_ai.try_borrow_mut_data()?, &o)?;
    Ok(())
}

fn resolve_tick(program_id: &Pubkey, accounts: &[AccountInfo], to: u8) -> ProgramResult {
    let chunk0 = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let world = world_chunks(program_id, accounts, world_season_id(chunk0)?)?;
    let mut meta = world.meta()?;
    if meta.finished {
        return Err(ChainError::WrongStatus.into());
    }
    let it = &mut accounts[WORLD_CHUNKS..].iter();
    let body = world.body(&WORLD_MAGIC)?;
    let pre_root = hashv(&[&body]).to_bytes();
    let mut state = permutation_rules::state::WorldState::try_from_slice(&body).map_err(|_| ChainError::WrongWorld)?;
    drop(body);
    let rules = rules_for(meta.preset)?;
    let mut order_ais = Vec::with_capacity(meta.civs as usize);
    let mut batches = Vec::new();
    for civ in 0..meta.civs as u16 {
        let ai = next_account_info(it)?;
        let o = load_orders(program_id, ai, meta.season_id, civ)?;
        if let Some(b) = o.batch.filter(|b| b.tick == state.tick) {
            batches.push(b);
        }
        order_ais.push(ai);
    }
    let clock = Clock::get()?;
    let everyone = batches.len() == meta.civs as usize;
    if state.phase_cursor == 0 && clock.unix_timestamp < meta.deadline && !everyone {
        return Err(ChainError::TooEarly.into());
    }
    // Tick randomness (§0.2): the committed history plus this slot. Only
    // phase 0 reads it. MagicBlock VRF is the planned replacement.
    let vrf = hashv(&[b"PS/tick-vrf/v1", &state.event_head, &clock.slot.to_le_bytes(), &clock.unix_timestamp.to_le_bytes()]).to_bytes();
    let tick = state.tick;
    let input = TickInput { vrf, batches };
    while state.phase_cursor < to.min(12) {
        let p = state.phase_cursor;
        run_phase(&mut state, &rules, &input, p).map_err(|_| ChainError::Rules)?;
        if state.phase_cursor == 0 {
            break; // the tick completed
        }
    }
    let completed = state.tick != tick;
    if completed {
        for (civ, ai) in order_ais.iter().enumerate() {
            let mut o: Orders = load(&ai.try_borrow_data()?)?;
            let c = &state.civs[civ];
            o.open_tick = state.tick;
            o.spendable = c.tick_budget as u32 + c.order_bank as u32;
            o.batch = None;
            store(&mut ai.try_borrow_mut_data()?, &o)?;
        }
        meta.deadline = clock.unix_timestamp + meta.tick_seconds as i64;
        meta.finished = state.tick >= rules.ticks_per_season;
    }
    let root = world.write_world(&state)?;
    world.set_meta(&meta)?;
    // Tick record for the replay verifier: everything needed to recompute this
    // step from the previous state, and the root it must produce.
    let batches = borsh::to_vec(&input.batches).map_err(|_| ChainError::Rules)?;
    solana_program::log::sol_log_data(&[b"PS_TICK", &tick.to_le_bytes(), &[to], &vrf, &pre_root, &root, &batches]);
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
    let rules = rules_for(season.preset)?;
    if state.tick < rules.ticks_per_season {
        return Err(ChainError::SeasonNotOver.into());
    }
    let p = permutation_rules::scoring::payouts(&rules, &state, season.pool);
    season.payouts = p.per_civ;
    season.claimed = vec![false; season.payouts.len()];
    season.rollover = p.rollover;
    season.final_root = state.state_root().map_err(|_| ChainError::Rules)?;
    season.status = SeasonStatus::Finalized;
    store(&mut season_ai.try_borrow_mut_data()?, &season)?;
    msg!("PS season {} finalized: payouts {:?} rollover {}", season.season_id, season.payouts, season.rollover);
    Ok(())
}

fn claim(program_id: &Pubkey, accounts: &[AccountInfo], civ: u16) -> ProgramResult {
    let it = &mut accounts.iter();
    let owner = next_account_info(it)?;
    let season_ai = next_account_info(it)?;
    let vault = next_account_info(it)?;
    let dest = next_account_info(it)?;
    let mint = next_account_info(it)?;
    let token_program = next_account_info(it)?;
    signer(owner)?;
    let mut season = load_season(program_id, season_ai)?;
    if season.status != SeasonStatus::Finalized {
        return Err(ChainError::WrongStatus.into());
    }
    let i = civ as usize;
    let slot = season.civs.get(i).ok_or(ChainError::InvalidParams)?;
    if owner.key.as_ref() != slot.payout {
        return Err(ChainError::Unauthorized.into());
    }
    if season.claimed[i] {
        return Err(ChainError::AlreadyClaimed.into());
    }
    let amount = season.payouts[i];
    if amount == 0 {
        return Err(ChainError::NothingToClaim.into());
    }
    if token_program.key != &token::TOKEN_PROGRAM_ID || mint.key.as_ref() != season.usdc_mint {
        return Err(ChainError::WrongMint.into());
    }
    let id = season.season_id.to_le_bytes();
    expect_pda(program_id, vault, &[VAULT_SEED, &id])?;
    let (dmint, downer) = token::token_account_mint_owner(dest).ok_or(ChainError::WrongTokenAccount)?;
    if dmint != *mint.key || downer != *owner.key {
        return Err(ChainError::WrongTokenAccount.into());
    }
    season.claimed[i] = true;
    store(&mut season_ai.try_borrow_mut_data()?, &season)?;
    token::transfer_checked(vault, mint, dest, season_ai, amount, season.usdc_decimals, Some(&[SEASON_SEED, &id, &[season.bump]]))?;
    msg!("PS claim season {} civ {} amount {}", season.season_id, civ, amount);
    Ok(())
}
