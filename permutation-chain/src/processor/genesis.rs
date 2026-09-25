//! Base layer, once registration closes: genesis, seating the members and
//! opening the government (tick 0).

use permutation_rules::genesis::{check_entries, nation_entries, season_from_map};
use permutation_rules::gov::{self, GovAction, GovEntry, Role, NOBODY};
use permutation_rules::map::{MapJob, MapStep};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint::ProgramResult,
    hash::hashv,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar::{slot_hashes, Sysvar},
};

use super::accounts::*;
use super::play::open_nation;
use crate::error::ChainError;
use crate::state::*;

pub(super) fn start_season(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let it = &mut accounts.iter();
    let authority = next_account_info(it)?;
    let season_ai = next_account_info(it)?;
    let hashes = next_account_info(it)?;
    signer(authority)?;
    let mut season = load_season(program_id, season_ai)?;
    require_operator(&season, authority)?;
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
    let recent = hashes
        .try_borrow_data()?
        .get(16..48)
        .map(|h| h.to_vec())
        .ok_or(ChainError::InvalidParams)?;
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
    let job = GenesisJob {
        world_seed: season.world_seed,
        season_seed: season.season_seed,
        entries,
        map,
    };
    world.write_genesis(&job)?;
    let meta = WorldMeta {
        season_id: season.season_id,
        preset: season.preset,
        civs: season.nations,
        tick_seconds: season.tick_seconds,
        deadline: 0,
        finished: false,
        market: season.market,
        ..Default::default()
    };
    world.set_meta(&meta)?;
    season.status = SeasonStatus::Genesis;
    store(&mut season_ai.try_borrow_mut_data()?, &season)?;
    msg!(
        "PS season {} genesis started: {} nations, {} members",
        season.season_id,
        season.nations,
        season.member_count
    );
    Ok(())
}

pub(super) fn genesis_step(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    work: u32,
) -> ProgramResult {
    let season_ai = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let mut season = load_season(program_id, season_ai)?;
    if season.status != SeasonStatus::Genesis {
        return Err(ChainError::WrongStatus.into());
    }
    let world = world_chunks(program_id, &accounts[1..], season.season_id)?;
    let rules = rules_for(season.preset, season.market)?;
    let GenesisJob {
        world_seed,
        season_seed,
        entries,
        map,
    } = world.read_genesis()?;
    match map
        .step(&rules, &world_seed, work)
        .map_err(|_| ChainError::Rules)?
    {
        MapStep::Working(map) => {
            msg!(
                "PS genesis attempt {} scanned {} rounds {}",
                map.attempt,
                map.scanned,
                map.rounds
            );
            world.write_genesis(&GenesisJob {
                world_seed,
                season_seed,
                entries,
                map,
            })
        }
        MapStep::Done(generated) => {
            let state = season_from_map(&rules, &world_seed, &season_seed, &entries, generated)
                .map_err(|_| ChainError::Rules)?;
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
pub(super) fn seat_members(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let authority = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let season_ai = accounts.get(1).ok_or(ProgramError::NotEnoughAccountKeys)?;
    signer(authority)?;
    let mut season = load_season(program_id, season_ai)?;
    require_operator(&season, authority)?;
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
        let mut entries = vec![GovEntry {
            member: id,
            signer: m.session,
            action: GovAction::Stand { roles: m.stand },
        }];
        for role in Role::ALL {
            let candidate = m.votes[role.index()];
            if candidate != NOBODY {
                entries.push(GovEntry {
                    member: id,
                    signer: m.session,
                    action: GovAction::Vote { role, candidate },
                });
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
    msg!(
        "PS seated {} members ({} of {})",
        seated.len(),
        season.seated,
        season.member_count
    );
    Ok(())
}

pub(super) fn open_government(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let authority = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let season_ai = accounts.get(1).ok_or(ProgramError::NotEnoughAccountKeys)?;
    signer(authority)?;
    let mut season = load_season(program_id, season_ai)?;
    require_operator(&season, authority)?;
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
