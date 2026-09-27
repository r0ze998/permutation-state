//! Base layer, once registration closes: the season seed, genesis, seating
//! the members and opening the government (tick 0).

use permutation_rules::genesis::{check_entries, map_seed, nation_entries, season_from_map};
use permutation_rules::gov;
use permutation_rules::map::{MapJob, MapStep};
use permutation_rules::state::WorldState;
use permutation_rules::Ruleset;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};

use super::accounts::*;
use super::play::open_nation;
use crate::error::ChainError;
use crate::randomness::{self, VRF_QUEUE_BASE};
use crate::seat::seat_member;
use crate::state::*;

/// The accounts of `StartSeason` and `RetrySeasonSeed` after the signer and
/// the season: our identity PDA, the base VRF queue, the VRF program, the
/// system program and the SlotHashes sysvar.
struct VrfAccounts<'a, 'info> {
    identity: &'a AccountInfo<'info>,
    queue: &'a AccountInfo<'info>,
    vrf: &'a AccountInfo<'info>,
    system: &'a AccountInfo<'info>,
    slot_hashes: &'a AccountInfo<'info>,
}

impl<'a, 'info> VrfAccounts<'a, 'info> {
    fn next(it: &mut std::slice::Iter<'a, AccountInfo<'info>>) -> Result<Self, ProgramError> {
        Ok(VrfAccounts {
            identity: next_account_info(it)?,
            queue: next_account_info(it)?,
            vrf: next_account_info(it)?,
            system: next_account_info(it)?,
            slot_hashes: next_account_info(it)?,
        })
    }

    /// The season seed is drawn on the base layer: only `VRF_QUEUE_BASE`
    /// (A20). Returns our identity's bump (`WrongOracle` on any mismatch).
    fn check(&self, program_id: &Pubkey) -> Result<u8, ProgramError> {
        check_vrf_accounts(
            program_id,
            self.identity,
            self.queue,
            self.vrf,
            self.system,
            self.slot_hashes,
            &VRF_QUEUE_BASE,
        )
    }
}

/// The registration totals the season seed binds (`randomness::season_seed_request`,
/// `randomness::season_seed`): fixed once registration closed, since
/// Register and UpdateMember need `Registering`.
fn treasury_borsh(season: &Season) -> Result<Vec<u8>, ProgramError> {
    borsh::to_vec(&season.treasury).map_err(|_| ChainError::InvalidParams.into())
}

/// The nation entries genesis builds the map for: the registered treasuries.
fn genesis_entries(season: &Season) -> Vec<permutation_rules::genesis::Entry> {
    let mut entries = nation_entries(season.nations as usize);
    for (e, t) in entries.iter_mut().zip(&season.treasury) {
        e.treasury = *t;
    }
    entries
}

/// `StartSeason`: close registration. The season moves to `Seeding`; it
/// makes no VRF request (A19): `RetrySeasonSeed` makes the first one, in
/// the same transaction as the crank sends them. The operator's bond must
/// reach `bond_floor` (WP09), and the season must be one genesis can build
/// (WP13, checked again: the rules may have changed since creation).
pub(super) fn start_season(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let it = &mut accounts.iter();
    let authority = next_account_info(it)?;
    let season_info = next_account_info(it)?;
    let vrf = VrfAccounts::next(it)?;
    signer(authority)?;
    let mut season = load_season(program_id, season_info)?;
    require_operator(&season, authority)?;
    if season.status != SeasonStatus::Registering || season.member_count == 0 {
        return Err(ChainError::WrongStatus.into());
    }
    require_rules(&season)?;
    if !creatable(season.preset, season.nations) {
        return Err(ChainError::InvalidParams.into());
    }
    if season.bond < bond_floor(&season) {
        return Err(ChainError::BondTooSmall.into());
    }
    // Validation only: a season whose genesis cannot start never leaves
    // registration (GenesisStep builds the job).
    let rules = rules_for(season.preset, season.market)?;
    let entries = genesis_entries(&season);
    check_entries(&rules, &entries).map_err(|_| ChainError::Rules)?;
    MapJob::new(&rules, entries.len()).map_err(|_| ChainError::Rules)?;
    let now = now()?;
    if cfg!(feature = "dev-randomness") {
        return start_dev(season_info, &mut season, vrf.slot_hashes, now);
    }
    vrf.check(program_id)?;
    season.status = SeasonStatus::Seeding;
    season.seed_state = randomness::SEED_PENDING;
    season.seed_requested_at = 0;
    season.seed_requests = 0;
    season.stage_at = now;
    store(&mut season_info.try_borrow_mut_data()?, &season)?;
    msg!(
        "PS season {} registration closed: {} nations, {} members (season seed pending)",
        season.season_id,
        season.nations,
        season.member_count
    );
    Ok(())
}

/// `dev-randomness` builds (local stacks without an oracle): the season
/// seed from the latest slot hash, in this transaction (the v6 formula,
/// `SEED_DEV`), and genesis opens at once. The verifier refuses such a seed
/// on public clusters.
#[inline(never)]
fn start_dev(
    season_info: &AccountInfo,
    season: &mut Season,
    hashes: &AccountInfo,
    now: i64,
) -> ProgramResult {
    if hashes.key != &solana_program::sysvar::slot_hashes::id() {
        return Err(ChainError::InvalidParams.into());
    }
    let recent = hashes
        .try_borrow_data()?
        .get(16..48)
        .map(|h| h.to_vec())
        .ok_or(ChainError::InvalidParams)?;
    let id = season.season_id.to_le_bytes();
    let count = season.member_count.to_le_bytes();
    let treasury = treasury_borsh(season)?;
    season.season_seed = solana_program::hash::hashv(&[
        b"PS/season-seed/v6",
        &recent,
        &id,
        &count,
        &treasury,
        &season.prev_history_root,
    ])
    .to_bytes();
    season.seed_state = randomness::SEED_DEV;
    season.status = SeasonStatus::Genesis;
    season.stage_at = now;
    store(&mut season_info.try_borrow_mut_data()?, season)?;
    solana_program::log::sol_log_data(&[
        b"PS_SEED",
        &id,
        &[randomness::SEED_DEV],
        &[0; 32],
        &season.season_seed,
    ]);
    msg!(
        "PS randomness dev: season {} seed from the slot hash",
        season.season_id
    );
    Ok(())
}

/// `RetrySeasonSeed`: while `Seeding`, request the season seed from the VRF
/// (the base queue): the first request after StartSeason, then again once
/// `SEED_RETRY_SECONDS` passed since the last one. Permissionless. The
/// caller seed binds the registration totals; the request is the last call.
pub(super) fn retry_season_seed(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let it = &mut accounts.iter();
    let payer = next_account_info(it)?;
    let season_info = next_account_info(it)?;
    let vrf = VrfAccounts::next(it)?;
    signer(payer)?;
    let mut season = load_season(program_id, season_info)?;
    if season.status != SeasonStatus::Seeding {
        return Err(ChainError::WrongStatus.into());
    }
    let now = now()?;
    if season.seed_requests > 0 && now < season.seed_requested_at.saturating_add(SEED_RETRY_SECONDS)
    {
        return Err(ChainError::TooEarly.into());
    }
    let bump = vrf.check(program_id)?;
    season.seed_requested_at = now;
    season.seed_requests = season.seed_requests.saturating_add(1);
    let caller_seed = randomness::season_seed_request(
        season.season_id,
        season.member_count,
        &treasury_borsh(&season)?,
        &season.prev_history_root,
    );
    store(&mut season_info.try_borrow_mut_data()?, &season)?;
    msg!(
        "PS season {} seed requested ({})",
        season.season_id,
        season.seed_requests
    );
    request_randomness(
        program_id,
        payer,
        vrf.identity,
        vrf.queue,
        vrf.vrf,
        vrf.system,
        vrf.slot_hashes,
        bump,
        caller_seed,
        randomness::CONSUME_SEED_TAG,
        season_info.key,
        &season.season_id.to_le_bytes(),
    )
}

/// `ConsumeSeasonSeed`: the VRF program's callback, signed as its scoped
/// identity for this program. Only a season still `Seeding` takes it (a
/// stale or second callback changes nothing and succeeds, so the oracle
/// never retries it): the season seed mixes the oracle's output into the
/// registration totals, and genesis opens.
pub(super) fn consume_season_seed(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    oracle: [u8; 32],
    season_id: u64,
) -> ProgramResult {
    let it = &mut accounts.iter();
    let identity = next_account_info(it)?;
    let season_info = next_account_info(it)?;
    if !identity.is_signer || identity.key != &randomness::scoped_vrf_identity(program_id) {
        return Err(ChainError::WrongOracle.into());
    }
    let mut season = load_season(program_id, season_info)?;
    if season.season_id != season_id {
        return Err(ChainError::WrongPda.into());
    }
    if season.status != SeasonStatus::Seeding || season.seed_state != randomness::SEED_PENDING {
        msg!("PS season {} seed callback ignored (stale)", season_id);
        return Ok(());
    }
    season.season_seed = randomness::season_seed(
        &oracle,
        season_id,
        season.member_count,
        &treasury_borsh(&season)?,
        &season.prev_history_root,
    );
    season.seed_oracle = oracle;
    season.seed_state = randomness::SEED_VRF;
    season.status = SeasonStatus::Genesis;
    season.stage_at = now()?;
    store(&mut season_info.try_borrow_mut_data()?, &season)?;
    solana_program::log::sol_log_data(&[
        b"PS_SEED",
        &season_id.to_le_bytes(),
        &[randomness::SEED_VRF],
        &oracle,
        &season.season_seed,
    ]);
    msg!("PS season {} seed drawn: genesis opens", season_id);
    Ok(())
}

/// `GenesisStep { work }`: the first call writes the genesis job and the
/// world meta; every later one advances the map. Permissionless, so the
/// caller picks `work`; the map does not depend on it, only the step's
/// compute does: clamped so every step fits one transaction (WP13).
pub(super) fn genesis_step(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    work: u32,
) -> ProgramResult {
    let work = work.clamp(1, MAX_GENESIS_WORK);
    let season_info = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let mut season = load_season(program_id, season_info)?;
    if season.status != SeasonStatus::Genesis {
        return Err(ChainError::WrongStatus.into());
    }
    require_rules(&season)?;
    let world = world_chunks(program_id, &accounts[1..], season.season_id)?;
    let magic = world.magic()?;
    if magic == WORLD_MAGIC {
        return Err(ChainError::WrongStatus.into());
    }
    let rules = rules_for(season.preset, season.market)?;
    if magic != GENESIS_MAGIC {
        return write_job(&world, &season, &rules);
    }
    let GenesisJob {
        world_seed,
        season_seed,
        entries,
        map,
    } = world.read_genesis()?;
    // The map seed needs the season seed, fixed only when the VRF answered
    // after registration closed (§2.4): nobody could compute the map
    // before choosing a nation.
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
            // The world generated here, in SBF, carries the hash the season
            // pinned (host-computed) (WP15).
            if state.ruleset_hash != season.rules_hash {
                return Err(ChainError::RulesMismatch.into());
            }
            let root = world.write_world(&state)?;
            season.status = SeasonStatus::Seating;
            season.stage_at = now()?;
            store(&mut season_info.try_borrow_mut_data()?, &season)?;
            solana_program::log::sol_log_data(&[b"PS_GENESIS", &root, &season_seed]);
            msg!("PS genesis complete");
            Ok(())
        }
    }
}

/// The first `GenesisStep`: the genesis job (the registered treasuries and
/// the seeds) and the world meta. Not inlined: the job stays out of
/// `genesis_step`'s frame.
#[inline(never)]
fn write_job(world: &Chunks, season: &Season, rules: &Ruleset) -> ProgramResult {
    let entries = genesis_entries(season);
    let map = MapJob::new(rules, entries.len()).map_err(|_| ChainError::Rules)?;
    world.write_genesis(&GenesisJob {
        world_seed: season.world_seed,
        season_seed: season.season_seed,
        entries,
        map,
    })?;
    world.set_meta(&WorldMeta {
        season_id: season.season_id,
        preset: season.preset,
        civs: season.nations,
        tick_seconds: season.tick_seconds,
        deadline: 0,
        finished: false,
        market: season.market,
        ..Default::default()
    })?;
    msg!(
        "PS genesis job written: season {}, {} nations, {} members",
        season.season_id,
        season.nations,
        season.member_count
    );
    Ok(())
}

/// Add the next members (registration order) to the world with their
/// pre-season candidacy and votes (`seat::seat_member`: a session key already
/// seated is replaced by a substitute, so a duplicate cannot stop seating;
/// votes count only in seasons without operator AI members). Logged as
/// `PS_SEAT ‖ root ‖ borsh(Vec<seat::Seat>)`, each with the key actually
/// seated, for the verifier. The operator's step; anyone's once it waited
/// `TAKEOVER_SECONDS` (WP14): it takes no input but the member accounts.
pub(super) fn seat_members(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let authority = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let season_info = accounts.get(1).ok_or(ProgramError::NotEnoughAccountKeys)?;
    signer(authority)?;
    let mut season = load_season(program_id, season_info)?;
    require_operator_or_overdue(&season, authority, now()?)?;
    require_rules(&season)?;
    if season.status != SeasonStatus::Seating {
        return Err(ChainError::WrongStatus.into());
    }
    let world = world_chunks(program_id, &accounts[2..], season.season_id)?;
    let rules = rules_for(season.preset, season.market)?;
    let pre_votes = season.ai_count == 0;
    let mut state = world.read_world()?;
    let mut seated = Vec::new();
    for info in &accounts[2 + WORLD_CHUNKS..] {
        let m = load_member(program_id, info, season.season_id)?;
        if m.index != state.members.len() as u32 {
            return Err(ChainError::InvalidParams.into());
        }
        let seat = seat_member(&mut state, &rules, &m, pre_votes).map_err(|_| ChainError::Rules)?;
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

/// `OpenGovernment`: once every member is seated, hold the first election
/// and open tick 0 on the nation accounts (with each nation's roll and
/// governance quota). Tick 0 closes `TICK0_GRACE_SECONDS` after a full tick,
/// so the operator can delegate first; its `StartClock` then gives tick 0 a
/// full tick on the layer that plays it (WP01). The operator's step;
/// anyone's once it waited `TAKEOVER_SECONDS` (WP14).
pub(super) fn open_government(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let authority = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let season_info = accounts.get(1).ok_or(ProgramError::NotEnoughAccountKeys)?;
    signer(authority)?;
    let mut season = load_season(program_id, season_info)?;
    let now = now()?;
    require_operator_or_overdue(&season, authority, now)?;
    require_rules(&season)?;
    if season.status != SeasonStatus::Seating || season.seated != season.member_count {
        return Err(ChainError::WrongStatus.into());
    }
    let deadline = now
        .saturating_add(season.tick_seconds as i64)
        .saturating_add(TICK0_GRACE_SECONDS);
    let root = open_world(program_id, &accounts[2..], &season, deadline)?;
    season.status = SeasonStatus::Running;
    season.stage_at = now;
    store(&mut season_info.try_borrow_mut_data()?, &season)?;
    solana_program::log::sol_log_data(&[b"PS_OPEN", &root]);
    msg!("PS government opened: tick 0 is open");
    Ok(())
}

/// OpenGovernment's world part: the first election, tick 0 in every nation
/// account, the world written and its meta reset for tick 0 (the previous
/// tick's flags and randomness cleared; `usdc_broken` is sticky). Returns
/// the world's root. Not inlined: the world stays out of
/// `open_government`'s 4 KiB stack frame, which holds the season.
#[inline(never)]
fn open_world(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    season: &Season,
    deadline: i64,
) -> Result<[u8; 32], ProgramError> {
    let world = world_chunks(program_id, accounts, season.season_id)?;
    let rules = rules_for(season.preset, season.market)?;
    let mut state = world.read_world()?;
    gov::first_election(&mut state, &rules).map_err(|_| ChainError::Rules)?;
    open_nations(
        program_id,
        &accounts[WORLD_CHUNKS..],
        season,
        &state,
        &rules,
        deadline,
    )?;
    let root = world.write_world(&state)?;
    let meta = world.meta()?;
    world.set_meta(&WorldMeta {
        deadline,
        finished: false,
        frozen: false,
        revealing: false,
        vrf: [0; 32],
        input_chunks: 0,
        input_logged: 0,
        input_hash: [0; 32],
        rand_state: randomness::RAND_NONE,
        rand_tick: NO_TICK,
        rand_pre: [0; 32],
        rand_out: [0; 32],
        frozen_at: 0,
        rand_requested_at: 0,
        rand_requests: 0,
        ..meta
    })?;
    Ok(root)
}

/// Opens tick 0 in every nation account, with its roll and quota. Not
/// inlined: the nation account stays out of the callers' 4 KiB frames.
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
        n.seat_roll(state);
        store(&mut info.try_borrow_mut_data()?, &n)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::processor::set_host_clock;
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

    fn account(space: usize, value: &impl borsh::BorshSerialize) -> Vec<u8> {
        let mut data = vec![0u8; space];
        store(&mut data, value).unwrap();
        data
    }

    /// A Seating season of `registered` members with genesis done:
    /// (program, operator, wallets, keys, account data), in the order
    /// operator, season, world chunks, members, nations.
    struct Fx {
        program: Pubkey,
        operator: Pubkey,
        outsider: Pubkey,
        wallets: Vec<Pubkey>,
        keys: Vec<Pubkey>,
        data: Vec<Vec<u8>>,
        lamports: Vec<u64>,
        members: usize,
    }

    /// (nation, session key, candidacy, General vote) per member.
    type Reg = (u16, [u8; 32], u8, u32);

    impl Fx {
        fn new(registered: &[Reg], ai_count: u16) -> Fx {
            let program = Pubkey::new_unique();
            let operator = Pubkey::new_unique();
            let id = ID.to_le_bytes();
            let rules = rules_for(0, true).unwrap();
            let wallets: Vec<Pubkey> = registered.iter().map(|_| Pubkey::new_unique()).collect();
            let mut keys = vec![operator];
            let mut data = vec![Vec::new()];
            let (season_key, bump) = Pubkey::find_program_address(&[SEASON_SEED, &id], &program);
            keys.push(season_key);
            let mut s = season(&operator, bump, registered.len() as u32);
            s.ai_count = ai_count;
            s.stage_at = 1_000;
            data.push(account(SEASON_SPACE, &s));
            for k in 0..WORLD_CHUNKS {
                keys.push(Pubkey::find_program_address(&[WORLD_SEED, &id, &[k as u8]], &program).0);
                data.push(vec![0u8; CHUNK]);
            }
            for (i, (civ, session, stand, vote)) in registered.iter().enumerate() {
                let wallet = wallets[i].to_bytes();
                let (key, bump) =
                    Pubkey::find_program_address(&[MEMBER_SEED, &id, &wallet], &program);
                let mut votes = [NOBODY; 4];
                votes[Role::General.index()] = *vote;
                let m = MemberAccount {
                    magic: MEMBER_MAGIC,
                    season_id: ID,
                    bump,
                    index: i as u32,
                    civ: *civ,
                    wallet,
                    session: *session,
                    kind: 2,
                    name: format!("member {i}"),
                    attestation: [0; 32],
                    stand: *stand,
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
            let outsider = Pubkey::new_unique();
            keys.push(outsider);
            data.push(Vec::new());
            let lamports = vec![0u64; keys.len()];
            let mut fx = Fx {
                program,
                operator,
                outsider,
                wallets,
                keys,
                data,
                lamports,
                members: registered.len(),
            };
            // Genesis is done: the world is written.
            let genesis = new_season(
                &rules,
                &[1; 32],
                &[2; 32],
                &nation_entries(NATIONS as usize),
            )
            .unwrap();
            fx.with(|infos| {
                let chunks = Chunks::new(&infos[2..2 + WORLD_CHUNKS]).unwrap();
                chunks
                    .set_meta(&WorldMeta {
                        season_id: ID,
                        civs: NATIONS as u8,
                        tick_seconds: TICK_SECONDS,
                        market: true,
                        ..Default::default()
                    })
                    .unwrap();
                chunks.write_world(&genesis).unwrap();
            });
            fx
        }

        /// Runs `f` on the accounts (the operator and the outsider sign).
        fn with<R>(&mut self, f: impl FnOnce(&[AccountInfo]) -> R) -> R {
            let (operator, outsider, program) = (self.operator, self.outsider, self.program);
            let infos: Vec<AccountInfo> = self
                .keys
                .iter()
                .zip(self.lamports.iter_mut())
                .zip(self.data.iter_mut())
                .map(|((k, l), d)| {
                    let person = *k == operator || *k == outsider;
                    let owner = if person { k } else { &program };
                    AccountInfo::new(k, person, true, l, d, owner, false)
                })
                .collect();
            f(&infos)
        }

        /// `signer`, the season and the world chunks, then `extra`
        /// (indices into the accounts).
        fn run(
            &mut self,
            signer: Pubkey,
            extra: std::ops::Range<usize>,
            ix: fn(&Pubkey, &[AccountInfo]) -> ProgramResult,
        ) -> ProgramResult {
            let program = self.program;
            self.with(|infos| {
                let who = infos.iter().find(|a| *a.key == signer).unwrap().clone();
                let list: Vec<AccountInfo> = std::iter::once(who)
                    .chain(infos[1..2 + WORLD_CHUNKS].iter().cloned())
                    .chain(infos[extra].iter().cloned())
                    .collect();
                ix(&program, &list)
            })
        }

        fn members(&self, r: std::ops::Range<usize>) -> std::ops::Range<usize> {
            2 + WORLD_CHUNKS + r.start..2 + WORLD_CHUNKS + r.end
        }

        fn nations(&self) -> std::ops::Range<usize> {
            let at = 2 + WORLD_CHUNKS + self.members;
            at..at + NATIONS as usize
        }

        fn season(&self) -> Season {
            load(&self.data[1]).unwrap()
        }

        fn nation(&self, civ: usize) -> NationAccount {
            load(&self.data[self.nations().start + civ]).unwrap()
        }

        fn world(&mut self) -> (WorldState, WorldMeta) {
            self.with(|infos| {
                let chunks = Chunks::new(&infos[2..2 + WORLD_CHUNKS]).unwrap();
                (chunks.read_world().unwrap(), chunks.meta().unwrap())
            })
        }

        fn edit_meta(&mut self, f: impl FnOnce(&mut WorldMeta)) {
            self.with(|infos| {
                let chunks = Chunks::new(&infos[2..2 + WORLD_CHUNKS]).unwrap();
                let mut meta = chunks.meta().unwrap();
                f(&mut meta);
                chunks.set_meta(&meta).unwrap();
            });
        }
    }

    /// Two members register one session key: both are seated (the second
    /// with its substitute key, with its candidacy and vote), and the
    /// government opens with the substitute cached as that office's key,
    /// each nation's roll listing the keys seated and its quota; tick 0
    /// closes a tick plus `TICK0_GRACE_SECONDS` after opening, and a meta
    /// left frozen or revealing is cleared.
    #[test]
    fn a_duplicate_session_key_no_longer_stops_seating() {
        let (session, other) = ([0xA1u8; 32], [0xB2u8; 32]);
        let registered = [
            (0u16, session, Role::Steward.bit(), NOBODY),
            (0, session, Role::General.bit(), 1),
            (1, other, 0x0f, NOBODY),
        ];
        let mut fx = Fx::new(&registered, 0);
        let op = fx.operator;
        set_host_clock(1_000);

        // Seated over two calls: the first holds the duplicate.
        fx.run(op, fx.members(0..2), seat_members).unwrap();
        fx.run(op, fx.members(2..3), seat_members).unwrap();
        assert_eq!(fx.season().seated, 3);
        let substitute = substitute_key(ID, &fx.wallets[1].to_bytes());
        let (world, _) = fx.world();
        let seated: Vec<[u8; 32]> = world.members.iter().map(|m| m.key).collect();
        assert_eq!(seated, vec![session, substitute, other]);
        assert_eq!(world.members[1].standing_for, Role::General.bit());

        // Stale flags from before (a meta is never frozen or revealing at
        // Open, but Open does not rely on it).
        fx.edit_meta(|m| {
            (m.frozen, m.revealing, m.finished) = (true, true, true);
            (m.vrf, m.input_hash, m.rand_state) = ([7; 32], [8; 32], 2);
            (m.input_chunks, m.input_logged, m.rand_requests) = (3, 2, 1);
            m.usdc_broken = true;
        });
        // OpenGovernment succeeds (it needs every member seated).
        set_host_clock(2_000);
        fx.run(op, fx.nations(), open_government).unwrap();
        set_host_clock(None);
        let s = fx.season();
        assert_eq!((s.status, s.stage_at), (SeasonStatus::Running, 2_000));
        let deadline = 2_000 + TICK_SECONDS as i64 + TICK0_GRACE_SECONDS;
        let (_, meta) = fx.world();
        assert_eq!(meta.deadline, deadline);
        assert!(!meta.frozen && !meta.revealing && !meta.finished);
        assert_eq!((meta.vrf, meta.input_hash), ([0; 32], [0; 32]));
        assert_eq!(
            (meta.rand_state, meta.rand_tick, meta.rand_requests),
            (0, NO_TICK, 0)
        );
        assert_eq!((meta.input_chunks, meta.input_logged), (0, 0));
        assert!(meta.usdc_broken, "sticky: OpenGovernment never clears it");
        let n0 = fx.nation(0);
        let (general, steward) = (Role::General.index(), Role::Steward.index());
        assert_eq!((n0.officers[general], n0.keys[general]), (1, substitute));
        assert_eq!((n0.officers[steward], n0.keys[steward]), (0, session));
        assert_eq!(n0.deadline, deadline);
        // The roll: the members of the nation with their seated keys.
        assert_eq!(
            n0.roll,
            vec![
                RollSeat {
                    member: 0,
                    key8: key8(&session)
                },
                RollSeat {
                    member: 1,
                    key8: key8(&substitute)
                },
            ]
        );
        assert_eq!(n0.gov_quota, gov_quota(3, 2));
        let n1 = fx.nation(1);
        assert_eq!((n1.officers[general], n1.keys[general]), (2, other));
        assert_eq!(
            n1.roll,
            vec![RollSeat {
                member: 2,
                key8: key8(&other)
            }]
        );
        assert_eq!((n1.gov_quota, n1.deadline), (gov_quota(3, 1), deadline));
    }

    /// Seating and opening are the operator's steps, and anyone's once the
    /// stage waited `TAKEOVER_SECONDS` (WP14): an outsider is refused at
    /// `stage_at + 599` and seats and opens at `+ 600`.
    #[test]
    fn an_outsider_seats_and_opens_after_the_takeover_time() {
        let registered = [(0u16, [1u8; 32], 0x0f, NOBODY), (1, [2; 32], 0x0f, NOBODY)];
        let mut fx = Fx::new(&registered, 0);
        let (out, stage) = (fx.outsider, fx.season().stage_at);
        let unauthorized: ProgramError = ChainError::Unauthorized.into();
        set_host_clock(stage + TAKEOVER_SECONDS - 1);
        assert_eq!(
            fx.run(out, fx.members(0..2), seat_members),
            Err(unauthorized.clone())
        );
        set_host_clock(stage + TAKEOVER_SECONDS);
        fx.run(out, fx.members(0..2), seat_members).unwrap();
        assert_eq!(fx.season().seated, 2);
        // Seating does not move the stage: the outsider may open too.
        fx.run(out, fx.nations(), open_government).unwrap();
        set_host_clock(None);
        assert_eq!(fx.season().status, SeasonStatus::Running);
        // Running: another OpenGovernment is refused, whoever sends it.
        set_host_clock(stage + 10 * TAKEOVER_SECONDS);
        assert_eq!(
            fx.run(out, fx.nations(), open_government),
            Err(ChainError::WrongStatus.into())
        );
        set_host_clock(None);
    }

    /// In a season with operator AI members the members' stored votes are
    /// not applied (WP10): a self-vote does not elect, the candidacy does.
    #[test]
    fn pre_season_votes_are_ignored_in_ai_seasons() {
        // Member 1 stands for General and votes for itself; member 0 stands
        // for General too and votes for nobody.
        let registered = [
            (0u16, [1u8; 32], Role::General.bit(), NOBODY),
            (0, [2; 32], Role::General.bit(), 1),
        ];
        for ai_count in [0u16, 1] {
            let mut fx = Fx::new(&registered, ai_count);
            let op = fx.operator;
            set_host_clock(1_000);
            fx.run(op, fx.members(0..2), seat_members).unwrap();
            // Seated: the candidacies stand either way; the vote only
            // without AIs.
            let (world, _) = fx.world();
            assert!(world
                .members
                .iter()
                .all(|m| m.standing_for == Role::General.bit()));
            let voted = if ai_count == 0 { 1 } else { NOBODY };
            assert_eq!(world.vote(1, Role::General), voted);
            assert_eq!(world.vote(0, Role::General), NOBODY);
            fx.run(op, fx.nations(), open_government).unwrap();
            set_host_clock(None);
            if ai_count == 0 {
                let (world, _) = fx.world();
                assert_eq!(world.nations[0].offices[Role::General.index()], 1);
            }
        }
    }

    /// A season created under other rules or settlement logic is refused by
    /// every seating step (WP15).
    #[test]
    fn seating_refuses_another_rules_hash() {
        let registered = [(0u16, [1u8; 32], 0x0f, NOBODY), (1, [2; 32], 0x0f, NOBODY)];
        let mut fx = Fx::new(&registered, 0);
        let op = fx.operator;
        let mut s = fx.season();
        s.rules_hash[0] ^= 1;
        store(&mut fx.data[1], &s).unwrap();
        set_host_clock(1_000);
        let mismatch: ProgramError = ChainError::RulesMismatch.into();
        assert_eq!(
            fx.run(op, fx.members(0..2), seat_members),
            Err(mismatch.clone())
        );
        assert_eq!(fx.run(op, fx.nations(), open_government), Err(mismatch));
        set_host_clock(None);
    }
}
