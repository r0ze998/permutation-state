//! Base layer, once registration closes: genesis, seating the members and
//! opening the government (tick 0).

use permutation_rules::genesis::{check_entries, map_seed, nation_entries, season_from_map};
use permutation_rules::gov;
use permutation_rules::map::{MapJob, MapStep};
use permutation_rules::state::WorldState;
use permutation_rules::Ruleset;
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
use crate::seat::seat_member;
use crate::state::*;

pub(super) fn start_season(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let it = &mut accounts.iter();
    let authority = next_account_info(it)?;
    let season_info = next_account_info(it)?;
    let hashes = next_account_info(it)?;
    signer(authority)?;
    let mut season = load_season(program_id, season_info)?;
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
    // The previous season's history root is part of it: this season's map
    // and randomness are built on its predecessor's recorded history.
    season.season_seed = hashv(&[
        b"PS/season-seed/v6",
        &recent,
        &id,
        &count,
        &treasury,
        &season.prev_history_root,
    ])
    .to_bytes();
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
    store(&mut season_info.try_borrow_mut_data()?, &season)?;
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
    let season_info = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let mut season = load_season(program_id, season_info)?;
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
    // The map seed needs the season seed, fixed only when registration
    // closed (§2.4): nobody could compute the map before choosing a nation.
    match map
        .step(&rules, &map_seed(&world_seed, &season_seed), work)
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
            store(&mut season_info.try_borrow_mut_data()?, &season)?;
            solana_program::log::sol_log_data(&[b"PS_GENESIS", &root, &season_seed]);
            msg!("PS genesis complete");
            Ok(())
        }
    }
}

/// Add the next members (registration order) to the world with their
/// pre-season candidacy and votes (`seat::seat_member`: a session key already
/// seated is replaced by a substitute, so a duplicate cannot stop seating).
/// Logged as `PS_SEAT ‖ root ‖ borsh(Vec<seat::Seat>)`, each with the key
/// actually seated, for the verifier.
pub(super) fn seat_members(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let authority = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let season_info = accounts.get(1).ok_or(ProgramError::NotEnoughAccountKeys)?;
    signer(authority)?;
    let mut season = load_season(program_id, season_info)?;
    require_operator(&season, authority)?;
    if season.status != SeasonStatus::Seating {
        return Err(ChainError::WrongStatus.into());
    }
    let world = world_chunks(program_id, &accounts[2..], season.season_id)?;
    let rules = rules_for(season.preset, season.market)?;
    let mut state = world.read_world()?;
    let mut seated = Vec::new();
    for info in &accounts[2 + WORLD_CHUNKS..] {
        let m = load_member(program_id, info, season.season_id)?;
        if m.index != state.members.len() as u32 {
            return Err(ChainError::InvalidParams.into());
        }
        let seat = seat_member(&mut state, &rules, &m).map_err(|_| ChainError::Rules)?;
        if seat.1 != m.session {
            msg!(
                "PS member {} seated with a substitute key (duplicate session key)",
                m.index
            );
        }
        seated.push(seat);
    }
    season.seated = state.members.len() as u32;
    let root = world.write_world(&state)?;
    store(&mut season_info.try_borrow_mut_data()?, &season)?;
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
    open_government_with(program_id, accounts, || Ok(Clock::get()?.unix_timestamp))
}

/// `OpenGovernment`, reading the clock through `now` (off chain, in unit
/// tests, there is no clock sysvar).
fn open_government_with(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    now: impl FnOnce() -> Result<i64, ProgramError>,
) -> ProgramResult {
    let authority = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let season_info = accounts.get(1).ok_or(ProgramError::NotEnoughAccountKeys)?;
    signer(authority)?;
    let mut season = load_season(program_id, season_info)?;
    require_operator(&season, authority)?;
    if season.status != SeasonStatus::Seating || season.seated != season.member_count {
        return Err(ChainError::WrongStatus.into());
    }
    let world = world_chunks(program_id, &accounts[2..], season.season_id)?;
    let rules = rules_for(season.preset, season.market)?;
    let mut state = world.read_world()?;
    gov::first_election(&mut state, &rules).map_err(|_| ChainError::Rules)?;
    let deadline = now()? + season.tick_seconds as i64;
    open_nations(
        program_id,
        &accounts[2 + WORLD_CHUNKS..],
        &season,
        &state,
        &rules,
        deadline,
    )?;
    let root = world.write_world(&state)?;
    let mut meta = world.meta()?;
    meta.deadline = deadline;
    world.set_meta(&meta)?;
    season.status = SeasonStatus::Running;
    store(&mut season_info.try_borrow_mut_data()?, &season)?;
    solana_program::log::sol_log_data(&[b"PS_OPEN", &root]);
    msg!("PS government opened: tick 0 is open");
    Ok(())
}

/// Opens tick 0 in every nation account. Not inlined: the nation account
/// stays out of `OpenGovernment`'s 4 KiB stack frame, which already holds
/// the season and the world.
#[inline(never)]
fn open_nations(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    season: &Season,
    state: &WorldState,
    rules: &Ruleset,
    deadline: i64,
) -> ProgramResult {
    let it = &mut accounts.iter();
    for civ in 0..season.nations as u16 {
        let info = next_account_info(it)?;
        let mut n = load_nation(program_id, info, season.season_id, civ)?;
        open_nation(&mut n, state, rules, deadline);
        store(&mut info.try_borrow_mut_data()?, &n)?;
    }
    Ok(())
}

/// `ConsumeSeasonSeed` (lands with unit P2; refused until then).
pub(super) fn consume_season_seed(
    _program_id: &Pubkey,
    _accounts: &[AccountInfo],
    _randomness: [u8; 32],
    _season_id: u64,
) -> ProgramResult {
    Err(ChainError::InvalidInstruction.into())
}

/// `RetrySeasonSeed` (lands with unit P2; refused until then).
pub(super) fn retry_season_seed(_program_id: &Pubkey, _accounts: &[AccountInfo]) -> ProgramResult {
    Err(ChainError::InvalidInstruction.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seat::substitute_key;
    use permutation_rules::genesis::new_season;
    use permutation_rules::gov::{Role, NOBODY};

    const ID: u64 = 7;
    const NATIONS: u16 = 2;
    const TICK_SECONDS: u32 = 30;

    fn season(operator: &Pubkey, bump: u8, members: u32) -> Season {
        Season {
            magic: SEASON_MAGIC,
            season_id: ID,
            bump,
            vault_bump: 0,
            admin: operator.to_bytes(),
            crank: operator.to_bytes(),
            usdc_mint: [0; 32],
            usdc_decimals: 6,
            preset: 0,
            nations: NATIONS as u8,
            entry_fee: 1,
            tick_seconds: TICK_SECONDS,
            market: true,
            status: SeasonStatus::Seating,
            world_seed: [1; 32],
            season_seed: [2; 32],
            member_count: members,
            nation_members: vec![0; NATIONS as usize],
            seated: 0,
            pool: 0,
            ops: 0,
            ops_withdrawn: false,
            treasury: vec![0; NATIONS as usize],
            treasury_final: Vec::new(),
            payouts: Vec::new(),
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
            rules_version: crate::rules::PINNED_RULES_VERSION,
            rules_hash: crate::rules::pinned_ruleset_hash(0, true).unwrap(),
            logic_version: crate::rules::CHAIN_LOGIC_VERSION,
            created_slot: 0,
        }
    }

    /// The operator, season and world chunks, then `extra`.
    fn accounts_with<'a>(
        head: &[AccountInfo<'a>],
        extra: &[AccountInfo<'a>],
    ) -> Vec<AccountInfo<'a>> {
        head.iter().chain(extra).cloned().collect()
    }

    fn account(space: usize, value: &impl borsh::BorshSerialize) -> Vec<u8> {
        let mut data = vec![0u8; space];
        store(&mut data, value).unwrap();
        data
    }

    /// Two members register one session key: both are seated (the second
    /// with its substitute key, with its candidacy and vote), and the
    /// government opens with the substitute cached as that office's key.
    #[test]
    fn a_duplicate_session_key_no_longer_stops_seating() {
        let program = Pubkey::new_unique();
        let operator = Pubkey::new_unique();
        let id = ID.to_le_bytes();
        let rules = rules_for(0, true).unwrap();
        let (session, other) = ([0xA1u8; 32], [0xB2u8; 32]);
        let wallets: Vec<Pubkey> = (0..3).map(|_| Pubkey::new_unique()).collect();
        // (nation, session key, candidacy, General vote)
        let registered = [
            (0u16, session, Role::Steward.bit(), NOBODY),
            (0, session, Role::General.bit(), 1),
            (1, other, 0x0f, NOBODY),
        ];

        // Accounts: operator, season, world chunks, members, nations.
        let mut keys = vec![operator];
        let mut data = vec![Vec::new()];
        let (season_key, bump) = Pubkey::find_program_address(&[SEASON_SEED, &id], &program);
        keys.push(season_key);
        data.push(account(
            SEASON_SPACE,
            &season(&operator, bump, registered.len() as u32),
        ));
        for k in 0..WORLD_CHUNKS {
            keys.push(Pubkey::find_program_address(&[WORLD_SEED, &id, &[k as u8]], &program).0);
            data.push(vec![0u8; CHUNK]);
        }
        for (i, (civ, session, stand, vote)) in registered.into_iter().enumerate() {
            let wallet = wallets[i].to_bytes();
            let (key, bump) = Pubkey::find_program_address(&[MEMBER_SEED, &id, &wallet], &program);
            let mut votes = [NOBODY; 4];
            votes[Role::General.index()] = vote;
            let m = MemberAccount {
                magic: MEMBER_MAGIC,
                season_id: ID,
                bump,
                index: i as u32,
                civ,
                wallet,
                session,
                kind: 2,
                name: format!("member {i}"),
                attestation: [0; 32],
                stand,
                votes,
                shares: 0,
                claimed: false,
                tag: [0; 32],
            };
            keys.push(key);
            data.push(account(MEMBER_SPACE, &m));
        }
        for civ in 0..NATIONS {
            let (key, bump) =
                Pubkey::find_program_address(&[NATION_SEED, &id, &civ.to_le_bytes()], &program);
            let n = NationAccount::new(ID, civ, bump, 0, true, operator.to_bytes());
            keys.push(key);
            data.push(account(NATION_SPACE, &n));
        }
        let mut lamports = vec![0u64; keys.len()];
        let infos: Vec<AccountInfo> = keys
            .iter()
            .zip(lamports.iter_mut())
            .zip(data.iter_mut())
            .enumerate()
            .map(|(i, ((k, l), d))| {
                let owner = if i == 0 { &operator } else { &program };
                AccountInfo::new(k, i == 0, true, l, d, owner, false)
            })
            .collect();
        let head = &infos[..2 + WORLD_CHUNKS];
        let members = &infos[2 + WORLD_CHUNKS..2 + WORLD_CHUNKS + registered.len()];
        let nations = &infos[2 + WORLD_CHUNKS + registered.len()..];
        let with = |extra| accounts_with(head, extra);

        // Genesis is done: the world is written.
        let chunks = Chunks::new(&head[2..]).unwrap();
        chunks
            .set_meta(&WorldMeta {
                season_id: ID,
                civs: NATIONS as u8,
                tick_seconds: TICK_SECONDS,
                market: true,
                ..Default::default()
            })
            .unwrap();
        let genesis = new_season(
            &rules,
            &[1; 32],
            &[2; 32],
            &nation_entries(NATIONS as usize),
        );
        chunks.write_world(&genesis.unwrap()).unwrap();

        // Seated over two calls: the first holds the duplicate.
        seat_members(&program, &with(&members[..2])).unwrap();
        seat_members(&program, &with(&members[2..])).unwrap();
        let s: Season = load(&infos[1].try_borrow_data().unwrap()).unwrap();
        assert_eq!(s.seated, 3);
        let substitute = substitute_key(ID, &wallets[1].to_bytes());
        let world = chunks.read_world().unwrap();
        let seated: Vec<[u8; 32]> = world.members.iter().map(|m| m.key).collect();
        assert_eq!(seated, vec![session, substitute, other]);
        assert_eq!(world.members[1].standing_for, Role::General.bit());

        // OpenGovernment succeeds (it needs every member seated).
        open_government_with(&program, &with(nations), || Ok(1_000)).unwrap();
        let s: Season = load(&infos[1].try_borrow_data().unwrap()).unwrap();
        assert_eq!(s.status, SeasonStatus::Running);
        assert_eq!(chunks.meta().unwrap().deadline, 1_000 + TICK_SECONDS as i64);
        let n0: NationAccount = load(&nations[0].try_borrow_data().unwrap()).unwrap();
        let (general, steward) = (Role::General.index(), Role::Steward.index());
        assert_eq!((n0.officers[general], n0.keys[general]), (1, substitute));
        assert_eq!((n0.officers[steward], n0.keys[steward]), (0, session));
        let n1: NationAccount = load(&nations[1].try_borrow_data().unwrap()).unwrap();
        assert_eq!((n1.officers[general], n1.keys[general]), (2, other));
    }
}
