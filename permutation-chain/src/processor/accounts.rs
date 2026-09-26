//! Account checks and loaders shared by every instruction, and the
//! program's clock (`now`).

use solana_program::{
    account_info::AccountInfo,
    clock::Clock,
    entrypoint::ProgramResult,
    program::{invoke, invoke_signed},
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    sysvar::Sysvar,
};
use solana_system_interface::{instruction as system_instruction, program as system_program};

use crate::error::ChainError;
use crate::randomness;
use crate::state::*;
use crate::token;

pub(super) use crate::rules::rules_for;

/// The compute-budget program: the only other program a tick-driving
/// transaction may carry (`require_alone`).
#[allow(dead_code)] // the handlers of wave 3 call it
pub(super) const COMPUTE_BUDGET_ID: Pubkey =
    solana_program::pubkey!("ComputeBudget111111111111111111111111111111");

#[cfg(all(not(target_os = "solana"), any(test, feature = "host-clock")))]
std::thread_local! {
    static HOST_CLOCK: core::cell::Cell<Option<i64>> = const { core::cell::Cell::new(None) };
}

/// Unix time: the Clock sysvar on chain. On the host, with `test` or the
/// `host-clock` feature, the time `set_host_clock` set (if any), so the
/// processor's host tests can drive deadlines.
pub fn now() -> Result<i64, ProgramError> {
    #[cfg(all(not(target_os = "solana"), any(test, feature = "host-clock")))]
    if let Some(t) = HOST_CLOCK.with(|c| c.get()) {
        return Ok(t);
    }
    Ok(Clock::get()?.unix_timestamp)
}

/// Host tests: the time `now` returns on this thread (`None`: the sysvar).
#[cfg(all(not(target_os = "solana"), any(test, feature = "host-clock")))]
pub fn set_host_clock(unix: impl Into<Option<i64>>) {
    let t = unix.into();
    HOST_CLOCK.with(|c| c.set(t));
}

/// A tick-driving instruction (CloseCommits, LogTickInput, ResolveTick,
/// UndelegatePart) must be the only instruction of its transaction besides
/// compute-budget ones, and must not run as a CPI: nobody can bundle it
/// with a submission that reads its outcome, or wrap it in a program that
/// aborts an unwanted result. `sysvar` is the Instructions sysvar account.
#[allow(dead_code)] // the handlers of wave 3 call it
#[allow(deprecated)]
pub(super) fn require_alone(program_id: &Pubkey, sysvar: Option<&AccountInfo>) -> ProgramResult {
    use solana_program::sysvar::instructions::{
        check_id, load_current_index_checked, load_instruction_at_checked,
    };
    let sysvar = sysvar.ok_or(ChainError::NotAlone)?;
    if !check_id(sysvar.key) {
        return Err(ChainError::NotAlone.into());
    }
    let current = load_current_index_checked(sysvar)? as usize;
    let count = {
        let d = sysvar.try_borrow_data()?;
        u16::from_le_bytes([
            *d.first().ok_or(ChainError::NotAlone)?,
            *d.get(1).ok_or(ChainError::NotAlone)?,
        ]) as usize
    };
    for i in 0..count {
        let ix = load_instruction_at_checked(i, sysvar)?;
        // The current one must be ours (not a CPI from another top-level program).
        let ok = if i == current {
            ix.program_id == *program_id
        } else {
            ix.program_id == COMPUTE_BUDGET_ID
        };
        if !ok {
            return Err(ChainError::NotAlone.into());
        }
    }
    Ok(())
}

pub(super) fn signer(a: &AccountInfo) -> ProgramResult {
    if a.is_signer {
        Ok(())
    } else {
        Err(ChainError::MissingSignature.into())
    }
}

pub(super) fn expect_pda(
    program_id: &Pubkey,
    account: &AccountInfo,
    seeds: &[&[u8]],
) -> Result<u8, ProgramError> {
    let (pda, bump) = Pubkey::find_program_address(seeds, program_id);
    if pda != *account.key {
        return Err(ChainError::WrongPda.into());
    }
    Ok(bump)
}

/// Claim a system-owned PDA even if a third party pre-funded it (defeats the
/// one-lamport squatting attack), then assign it to `owner`.
pub(super) fn init_pda<'a>(
    payer: &AccountInfo<'a>,
    pda: &AccountInfo<'a>,
    space: usize,
    owner: &Pubkey,
    seeds: &[&[u8]],
) -> ProgramResult {
    if pda.owner != &system_program::id() || !pda.data_is_empty() {
        return Err(ChainError::AlreadyInitialized.into());
    }
    let need = Rent::get()?
        .minimum_balance(space)
        .saturating_sub(pda.lamports());
    if need > 0 {
        invoke(
            &system_instruction::transfer(payer.key, pda.key, need),
            &[payer.clone(), pda.clone()],
        )?;
    }
    invoke_signed(
        &system_instruction::allocate(pda.key, space as u64),
        std::slice::from_ref(pda),
        &[seeds],
    )?;
    invoke_signed(
        &system_instruction::assign(pda.key, owner),
        std::slice::from_ref(pda),
        &[seeds],
    )
}

pub(super) fn season_seeds(id: &[u8; 8]) -> [&[u8]; 2] {
    [SEASON_SEED, id]
}

pub(super) fn load_season(
    program_id: &Pubkey,
    account: &AccountInfo,
) -> Result<Season, ProgramError> {
    let s = load_season_any(program_id, account)?;
    if s.magic != SEASON_MAGIC {
        return Err(ChainError::NotInitialized.into());
    }
    Ok(s)
}

/// A season account of this version or of the previous one
/// (`LEGACY_SEASON_MAGIC`). Every field added since is appended, so a legacy
/// account's prefix decodes unchanged and its zero tail reads as 0/false
/// (`load` does not require the whole account to be consumed). Returns
/// (season, legacy). Only for CreateSeason's previous season, Claim and
/// WithdrawOps; everything else uses `load_season`.
#[allow(dead_code)] // the handlers of wave 3 call it
pub(super) fn load_season_compat(
    program_id: &Pubkey,
    account: &AccountInfo,
) -> Result<(Season, bool), ProgramError> {
    let s = load_season_any(program_id, account)?;
    let legacy = s.magic == LEGACY_SEASON_MAGIC;
    Ok((s, legacy))
}

/// A program-owned season PDA holding either magic. Not inlined: the
/// decoder's temporaries stay out of the callers' 4 KiB stack frames.
#[inline(never)]
fn load_season_any(program_id: &Pubkey, account: &AccountInfo) -> Result<Season, ProgramError> {
    if account.owner != program_id {
        return Err(ChainError::NotInitialized.into());
    }
    let s: Season = load(&account.try_borrow_data()?)?;
    if s.magic != SEASON_MAGIC && s.magic != LEGACY_SEASON_MAGIC {
        return Err(ChainError::NotInitialized.into());
    }
    expect_pda(
        program_id,
        account,
        &season_seeds(&s.season_id.to_le_bytes()),
    )?;
    Ok(s)
}

/// The season was created under the rules and settlement logic this build
/// runs (`rules::PINNED_RULESET_HASHES`, `rules::CHAIN_LOGIC_VERSION`).
#[allow(dead_code)] // the handlers of wave 3 call it
pub(super) fn require_rules(season: &Season) -> ProgramResult {
    if season.rules_hash != crate::rules::pinned_ruleset_hash(season.preset, season.market)?
        || season.logic_version != crate::rules::CHAIN_LOGIC_VERSION
    {
        return Err(ChainError::RulesMismatch.into());
    }
    Ok(())
}

/// A nation account, checked to be the PDA `["nation", season, civ]` it
/// claims to be. The owner and magic alone are not enough: world chunks after
/// the first are program-owned raw bytes, partly shaped by player input.
/// Uses the stored bump, so it costs one hash rather than a bump search.
pub(super) fn load_nation_at(
    program_id: &Pubkey,
    account: &AccountInfo,
) -> Result<NationAccount, ProgramError> {
    if account.owner != program_id {
        return Err(ChainError::MissingNation.into());
    }
    let n: NationAccount = load(&account.try_borrow_data()?)?;
    if n.magic != NATION_MAGIC {
        return Err(ChainError::MissingNation.into());
    }
    let seeds: [&[u8]; 4] = [
        NATION_SEED,
        &n.season_id.to_le_bytes(),
        &n.civ.to_le_bytes(),
        &[n.bump],
    ];
    if Pubkey::create_program_address(&seeds, program_id)
        .ok()
        .as_ref()
        != Some(account.key)
    {
        return Err(ChainError::WrongPda.into());
    }
    Ok(n)
}

/// The fixed front of a nation account (`NationHead`), with the checks of
/// `load_nation_at`: decodes no batch or inbox, so it needs no heap.
pub(super) fn load_nation_head_at(
    program_id: &Pubkey,
    account: &AccountInfo,
) -> Result<NationHead, ProgramError> {
    if account.owner != program_id {
        return Err(ChainError::MissingNation.into());
    }
    let n: NationHead = load(&account.try_borrow_data()?)?;
    if n.magic != NATION_MAGIC {
        return Err(ChainError::MissingNation.into());
    }
    let seeds: [&[u8]; 4] = [
        NATION_SEED,
        &n.season_id.to_le_bytes(),
        &n.civ.to_le_bytes(),
        &[n.bump],
    ];
    if Pubkey::create_program_address(&seeds, program_id)
        .ok()
        .as_ref()
        != Some(account.key)
    {
        return Err(ChainError::WrongPda.into());
    }
    Ok(n)
}

/// The nation account of `civ` in `season_id`.
pub(super) fn load_nation(
    program_id: &Pubkey,
    account: &AccountInfo,
    season_id: u64,
    civ: u16,
) -> Result<NationAccount, ProgramError> {
    let n = load_nation_at(program_id, account)?;
    if n.season_id != season_id || n.civ != civ {
        return Err(ChainError::MissingNation.into());
    }
    Ok(n)
}

pub(super) fn load_member(
    program_id: &Pubkey,
    account: &AccountInfo,
    season_id: u64,
) -> Result<MemberAccount, ProgramError> {
    if account.owner != program_id {
        return Err(ChainError::NotInitialized.into());
    }
    let m: MemberAccount = load(&account.try_borrow_data()?)?;
    if m.magic != MEMBER_MAGIC || m.season_id != season_id {
        return Err(ChainError::NotInitialized.into());
    }
    expect_pda(
        program_id,
        account,
        &[MEMBER_SEED, &season_id.to_le_bytes(), &m.wallet],
    )?;
    Ok(m)
}

/// The roster account `["roster", season]` (V5 §18.2).
pub(super) fn load_roster(
    program_id: &Pubkey,
    account: &AccountInfo,
    season_id: u64,
) -> Result<RosterAccount, ProgramError> {
    if account.owner != program_id {
        return Err(ChainError::NotInitialized.into());
    }
    let r: RosterAccount = load(&account.try_borrow_data()?)?;
    if r.magic != ROSTER_MAGIC || r.season_id != season_id {
        return Err(ChainError::NotInitialized.into());
    }
    expect_pda(
        program_id,
        account,
        &[ROSTER_SEED, &season_id.to_le_bytes()],
    )?;
    Ok(r)
}

/// The `WORLD_CHUNKS` world accounts at the front of `accounts`, checked.
pub(super) fn world_chunks<'a, 'info>(
    program_id: &Pubkey,
    accounts: &'a [AccountInfo<'info>],
    season_id: u64,
) -> Result<Chunks<'a, 'info>, ProgramError> {
    let list = accounts
        .get(..WORLD_CHUNKS)
        .ok_or(ChainError::WorldTooSmall)?;
    let id = season_id.to_le_bytes();
    for (k, a) in list.iter().enumerate() {
        if a.owner != program_id {
            return Err(ChainError::WrongWorld.into());
        }
        expect_pda(program_id, a, &[WORLD_SEED, &id, &[k as u8]])?;
    }
    Chunks::new(list)
}

/// The world accounts at the front of `accounts`, for the season chunk 0
/// names (the ER instructions carry no season account).
pub(super) fn world_at<'a, 'info>(
    program_id: &Pubkey,
    accounts: &'a [AccountInfo<'info>],
) -> Result<Chunks<'a, 'info>, ProgramError> {
    let chunk0 = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    world_chunks(program_id, accounts, world_season_id(chunk0)?)
}

/// The meta of world chunk 0 alone (instructions that take no other
/// chunk): program-owned, `CHUNK` long, `WORLD_MAGIC`, the PDA of the
/// season its header names.
#[allow(dead_code)] // the handlers of wave 3 call it
pub(super) fn world_chunk0(
    program_id: &Pubkey,
    chunk0: &AccountInfo,
) -> Result<WorldMeta, ProgramError> {
    if chunk0.owner != program_id || chunk0.data_len() != CHUNK {
        return Err(ChainError::WrongWorld.into());
    }
    if chunk0.try_borrow_data()?[..8] != WORLD_MAGIC {
        return Err(ChainError::WrongWorld.into());
    }
    let id = world_season_id(chunk0)?.to_le_bytes();
    expect_pda(program_id, chunk0, &[WORLD_SEED, &id, &[0]])?;
    WorldMeta::from_chunk0(&chunk0.try_borrow_data()?)
}

/// Season id recorded in world chunk 0 (to find the other accounts).
pub(super) fn world_season_id(chunk0: &AccountInfo) -> Result<u64, ProgramError> {
    let d = chunk0.try_borrow_data()?;
    Ok(u64::from_le_bytes(
        d.get(12..20)
            .ok_or(ChainError::WorldTooSmall)?
            .try_into()
            .unwrap(),
    ))
}

/// Only the season's crank may commit during play: MagicBlock sponsors a
/// limited number of commits per delegated account, and the last one is
/// kept for the final undelegation. Every nation account of the season
/// records the crank's key, and they stay delegated for the whole of play.
pub(super) fn require_crank(
    program_id: &Pubkey,
    nation_info: &AccountInfo,
    season_id: u64,
    payer: &AccountInfo,
) -> ProgramResult {
    let n = load_nation_head_at(program_id, nation_info)?;
    if n.season_id != season_id {
        return Err(ChainError::MissingNation.into());
    }
    if payer.key.as_ref() != n.crank {
        return Err(ChainError::Unauthorized.into());
    }
    Ok(())
}

/// The season's admin or crank: they run genesis, seating and delegation.
pub(super) fn require_operator(season: &Season, authority: &AccountInfo) -> ProgramResult {
    let key = authority.key.as_ref();
    if key == season.admin || key == season.crank {
        Ok(())
    } else {
        Err(ChainError::Unauthorized.into())
    }
}

/// The operator, or anyone once the stage is overdue
/// (`lifecycle::overdue`): seating and opening the government never wait
/// on the operator.
#[allow(dead_code)] // the handlers of wave 3 call it
pub(super) fn require_operator_or_overdue(
    season: &Season,
    authority: &AccountInfo,
    now: i64,
) -> ProgramResult {
    if require_operator(season, authority).is_ok() || crate::lifecycle::overdue(season, now) {
        Ok(())
    } else {
        Err(ChainError::Unauthorized.into())
    }
}

/// The accounts of a VRF request: the VRF program (executable), exactly
/// `expected_queue` (writable), the system program, the SlotHashes sysvar
/// and our identity PDA. Returns the identity's bump; any mismatch is
/// `WrongOracle`. The caller names one queue, picked from the play mode the
/// chain recorded (A20: a delegated season's ticks use `VRF_QUEUE_ER`,
/// base play and the season seed `VRF_QUEUE_BASE`), never both: a request
/// filed in the other layer's queue is never served, and a crank that could
/// file one would win every retry race and reach the fallback.
#[allow(dead_code)] // the handlers of wave 3 call it
pub(super) fn check_vrf_accounts(
    program_id: &Pubkey,
    identity: &AccountInfo,
    queue: &AccountInfo,
    vrf_program: &AccountInfo,
    system: &AccountInfo,
    slot_hashes: &AccountInfo,
    expected_queue: &Pubkey,
) -> Result<u8, ProgramError> {
    let (pda, bump) = randomness::program_identity(program_id);
    let ok = vrf_program.key == &randomness::VRF_PROGRAM_ID
        && vrf_program.executable
        && queue.key == expected_queue
        && queue.is_writable
        && system.key == &system_program::id()
        && slot_hashes.key == &solana_program::sysvar::slot_hashes::id()
        && identity.key == &pda;
    if !ok {
        return Err(ChainError::WrongOracle.into());
    }
    Ok(bump)
}

/// Request randomness from the VRF (`randomness::request_ix`), signed by our
/// identity PDA; the oracle calls back with variant `tag`, writing
/// `callback_account`. The accounts were checked by `check_vrf_accounts`.
/// The last call of its instruction (no borrow may be held).
#[allow(dead_code)] // the handlers of wave 3 call it
#[allow(clippy::too_many_arguments)]
pub(super) fn request_randomness<'info>(
    program_id: &Pubkey,
    payer: &AccountInfo<'info>,
    identity: &AccountInfo<'info>,
    queue: &AccountInfo<'info>,
    vrf_program: &AccountInfo<'info>,
    system: &AccountInfo<'info>,
    slot_hashes: &AccountInfo<'info>,
    bump: u8,
    caller_seed: [u8; 32],
    tag: u8,
    callback_account: &Pubkey,
    args: &[u8],
) -> ProgramResult {
    let ix = randomness::request_ix(
        program_id,
        payer.key,
        queue.key,
        caller_seed,
        tag,
        callback_account,
        args,
    );
    invoke_signed(
        &ix,
        &[
            payer.clone(),
            identity.clone(),
            queue.clone(),
            system.clone(),
            slot_hashes.clone(),
            vrf_program.clone(),
        ],
        &[&[randomness::IDENTITY_SEED, &[bump]]],
    )
}

/// The season's vault and its SPL amount: the vault PDA, a token account
/// of the season's mint owned by the season (else `WrongTokenAccount`).
#[allow(dead_code)] // the handlers of wave 3 call it
pub(super) fn vault_amount(
    program_id: &Pubkey,
    season: &Season,
    season_key: &Pubkey,
    vault: &AccountInfo,
) -> Result<u64, ProgramError> {
    expect_pda(
        program_id,
        vault,
        &[VAULT_SEED, &season.season_id.to_le_bytes()],
    )?;
    let (mint, owner) =
        token::token_account_mint_owner(vault).ok_or(ChainError::WrongTokenAccount)?;
    if mint.as_ref() != season.usdc_mint || owner != *season_key {
        return Err(ChainError::WrongTokenAccount.into());
    }
    Ok(token::token_account_amount(vault).ok_or(ChainError::WrongTokenAccount)?)
}

/// The season's USDC mint, the token program and the season's vault.
pub(super) fn check_usdc(
    program_id: &Pubkey,
    season: &Season,
    mint: &AccountInfo,
    token_program: &AccountInfo,
    vault: &AccountInfo,
) -> ProgramResult {
    if token_program.key != &token::TOKEN_PROGRAM_ID || mint.key.as_ref() != season.usdc_mint {
        return Err(ChainError::WrongMint.into());
    }
    expect_pda(
        program_id,
        vault,
        &[VAULT_SEED, &season.season_id.to_le_bytes()],
    )?;
    Ok(())
}

/// A token account of `mint` (and, with `owner`, owned by it).
pub(super) fn check_token_account(
    account: &AccountInfo,
    mint: &AccountInfo,
    owner: Option<&Pubkey>,
) -> ProgramResult {
    let (m, o) = token::token_account_mint_owner(account).ok_or(ChainError::WrongTokenAccount)?;
    if m != *mint.key || owner.is_some_and(|w| o != *w) {
        return Err(ChainError::WrongTokenAccount.into());
    }
    Ok(())
}

/// Pay `amount` out of the season's vault, signed by the season account.
pub(super) fn pay_from_vault<'info>(
    season: &Season,
    season_info: &AccountInfo<'info>,
    vault: &AccountInfo<'info>,
    mint: &AccountInfo<'info>,
    dest: &AccountInfo<'info>,
    amount: u64,
) -> ProgramResult {
    let id = season.season_id.to_le_bytes();
    token::transfer_checked(
        vault,
        mint,
        dest,
        season_info,
        amount,
        season.usdc_decimals,
        Some(&[SEASON_SEED, &id, &[season.bump]]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nation(season_id: u64, civ: u16, bump: u8) -> Vec<u8> {
        let n = NationAccount::new(season_id, civ, bump, 0, false, [0; 32]);
        let mut data = vec![0u8; NATION_SPACE];
        store(&mut data, &n).unwrap();
        data
    }

    #[test]
    fn only_the_crank_commits() {
        let program = Pubkey::new_unique();
        let (pda, bump) = Pubkey::find_program_address(
            &[NATION_SEED, &7u64.to_le_bytes(), &0u16.to_le_bytes()],
            &program,
        );
        let crank = Pubkey::new_unique();
        let mut n: NationAccount = load(&nation(7, 0, bump)).unwrap();
        n.crank = crank.to_bytes();
        let mut data = vec![0u8; NATION_SPACE];
        store(&mut data, &n).unwrap();
        let (mut l, mut l1, mut l2) = (0u64, 0u64, 0u64);
        let nation_info = AccountInfo::new(&pda, false, false, &mut l, &mut data, &program, false);
        let mut none: [u8; 0] = [];
        let mut none2: [u8; 0] = [];
        let owner = Pubkey::default();
        let stranger_key = Pubkey::new_unique();
        let crank_info = AccountInfo::new(&crank, true, true, &mut l1, &mut none, &owner, false);
        let stranger = AccountInfo::new(
            &stranger_key,
            true,
            true,
            &mut l2,
            &mut none2,
            &owner,
            false,
        );
        assert!(require_crank(&program, &nation_info, 7, &crank_info).is_ok());
        assert_eq!(
            require_crank(&program, &nation_info, 7, &stranger).unwrap_err(),
            ChainError::Unauthorized.into()
        );
        assert_eq!(
            require_crank(&program, &nation_info, 8, &crank_info).unwrap_err(),
            ChainError::MissingNation.into()
        );
    }

    /// A program-owned account holding a well-formed nation is not a nation
    /// unless it is that nation's PDA (world chunks are program-owned too).
    #[test]
    fn nations_are_checked_by_address() {
        let program = Pubkey::new_unique();
        let (pda, bump) = Pubkey::find_program_address(
            &[NATION_SEED, &7u64.to_le_bytes(), &2u16.to_le_bytes()],
            &program,
        );
        let impostor = Pubkey::new_unique();
        let (mut l1, mut l2) = (0u64, 0u64);
        let (mut d1, mut d2) = (nation(7, 2, bump), nation(7, 2, bump));
        let real = AccountInfo::new(&pda, false, true, &mut l1, &mut d1, &program, false);
        let fake = AccountInfo::new(&impostor, false, true, &mut l2, &mut d2, &program, false);
        assert!(load_nation(&program, &real, 7, 2).is_ok());
        assert_eq!(
            load_nation(&program, &fake, 7, 2).unwrap_err(),
            ChainError::WrongPda.into()
        );
        assert_eq!(
            load_nation(&program, &real, 7, 3).unwrap_err(),
            ChainError::MissingNation.into()
        );
        let other = Pubkey::new_unique();
        let (mut l3, mut d3) = (0u64, nation(7, 2, bump));
        let foreign = AccountInfo::new(&pda, false, true, &mut l3, &mut d3, &other, false);
        assert_eq!(
            load_nation_at(&program, &foreign).unwrap_err(),
            ChainError::MissingNation.into()
        );
    }

    fn season_fx(id: u64, bump: u8) -> Season {
        Season {
            magic: SEASON_MAGIC,
            season_id: id,
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
            history_root: [4; 32],
            ai_count: 0,
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
            stage_at: 1_000,
            rolled_back: 0,
            aborted_from: 0,
            validator: [5; 32],
            rules_version: crate::rules::PINNED_RULES_VERSION,
            rules_hash: crate::rules::pinned_ruleset_hash(PRESET_BLITZ, true).unwrap(),
            logic_version: crate::rules::CHAIN_LOGIC_VERSION,
            created_slot: 0,
        }
    }

    /// An account holding `data`.
    fn info<'a>(
        key: &'a Pubkey,
        owner: &'a Pubkey,
        lamports: &'a mut u64,
        data: &'a mut [u8],
    ) -> AccountInfo<'a> {
        AccountInfo::new(key, false, true, lamports, data, owner, false)
    }

    mod alone {
        #![allow(deprecated)]
        use super::super::*;
        use solana_program::instruction::{AccountMeta, Instruction};
        use solana_program::sysvar::instructions::{
            construct_instructions_data, BorrowedAccountMeta, BorrowedInstruction,
        };

        /// The Instructions sysvar data for `ixs`, with `current` as the executing index.
        fn sysvar_data(ixs: &[Instruction], current: u16) -> Vec<u8> {
            let borrowed: Vec<BorrowedInstruction> = ixs
                .iter()
                .map(|ix| BorrowedInstruction {
                    program_id: &ix.program_id,
                    accounts: ix
                        .accounts
                        .iter()
                        .map(|m| BorrowedAccountMeta {
                            pubkey: &m.pubkey,
                            is_signer: m.is_signer,
                            is_writable: m.is_writable,
                        })
                        .collect(),
                    data: &ix.data,
                })
                .collect();
            let mut d = construct_instructions_data(&borrowed);
            let n = d.len();
            d[n - 2..].copy_from_slice(&current.to_le_bytes());
            d
        }

        fn check_at(
            key: &Pubkey,
            program: &Pubkey,
            ixs: &[Instruction],
            current: u16,
        ) -> ProgramResult {
            let mut data = sysvar_data(ixs, current);
            let mut lamports = 0u64;
            let owner = Pubkey::default();
            let info = AccountInfo::new(key, false, false, &mut lamports, &mut data, &owner, false);
            require_alone(program, Some(&info))
        }

        fn check(program: &Pubkey, ixs: &[Instruction], current: u16) -> ProgramResult {
            check_at(
                &solana_program::sysvar::instructions::ID,
                program,
                ixs,
                current,
            )
        }

        #[test]
        fn tick_instructions_must_be_alone() {
            let program = Pubkey::new_unique();
            let ours = Instruction {
                program_id: program,
                accounts: vec![AccountMeta::new(Pubkey::new_unique(), false)],
                data: vec![19, 0, 0],
            };
            let cb = Instruction {
                program_id: COMPUTE_BUDGET_ID,
                accounts: vec![],
                data: vec![2, 0, 0, 0, 0],
            };
            let other = Instruction {
                program_id: Pubkey::new_unique(),
                accounts: vec![],
                data: vec![],
            };
            assert!(check(&program, std::slice::from_ref(&ours), 0).is_ok());
            assert!(check(&program, &[cb.clone(), cb.clone(), ours.clone()], 2).is_ok());
            let not_alone: ProgramError = ChainError::NotAlone.into();
            assert_eq!(
                check(&program, &[ours.clone(), ours.clone()], 0).unwrap_err(),
                not_alone
            );
            assert_eq!(
                check(&program, &[cb.clone(), ours.clone(), other.clone()], 1).unwrap_err(),
                not_alone
            );
            // Called through another top-level program (CPI): the current
            // instruction is not ours.
            assert_eq!(
                check(&program, std::slice::from_ref(&other), 0).unwrap_err(),
                not_alone
            );
            assert_eq!(require_alone(&program, None).unwrap_err(), not_alone);
            // Another account in the sysvar's place.
            assert_eq!(
                check_at(&Pubkey::new_unique(), &program, &[ours], 0).unwrap_err(),
                not_alone
            );
        }
    }

    #[test]
    fn require_rules_refuses_another_hash_or_logic() {
        let s = season_fx(7, 0);
        assert!(require_rules(&s).is_ok());
        let mut other = s.clone();
        other.rules_hash[0] ^= 1;
        assert_eq!(
            require_rules(&other).unwrap_err(),
            ChainError::RulesMismatch.into()
        );
        let mut other = s.clone();
        other.logic_version += 1;
        assert_eq!(
            require_rules(&other).unwrap_err(),
            ChainError::RulesMismatch.into()
        );
        // The market is part of the ruleset.
        let mut other = s.clone();
        other.market = false;
        assert_eq!(
            require_rules(&other).unwrap_err(),
            ChainError::RulesMismatch.into()
        );
        let mut other = s.clone();
        other.preset = 2;
        assert_eq!(
            require_rules(&other).unwrap_err(),
            ChainError::InvalidParams.into()
        );
    }

    /// A season account written by the previous program (`PSSEASN7`: the
    /// fields up to `bounty_paid`, the rest of the 4 KiB zero) loads only
    /// through the compat loader, flagged legacy.
    #[test]
    fn load_season_compat_reads_a_v7_account() {
        let program = Pubkey::new_unique();
        let (key, bump) =
            Pubkey::find_program_address(&[SEASON_SEED, &7u64.to_le_bytes()], &program);
        let mut v7 = season_fx(7, bump);
        v7.magic = LEGACY_SEASON_MAGIC;
        v7.status = SeasonStatus::Finalized;
        v7.payouts = vec![5, 6];
        v7.treasury_final = vec![1, 2];
        v7.bounty_paid = vec![0, 0];
        // The v7 layout is the v8 one without the appended tail.
        let full = borsh::to_vec(&v7).unwrap();
        let tail = 4
            + 32
            + 4
            + 4
            + (1 + 32 + 8 + 1)
            + (8 + 8 + 1)
            + (8 + 8 + 4 + 1 + 32)
            + (2 + 32 + 2 + 8);
        let mut data = vec![0u8; SEASON_SPACE];
        data[..full.len() - tail].copy_from_slice(&full[..full.len() - tail]);
        let mut lamports = 0;
        let account = info(&key, &program, &mut lamports, &mut data);
        let (s, legacy) = load_season_compat(&program, &account).unwrap();
        assert!(legacy);
        assert_eq!(
            (s.status, s.payouts.clone(), s.history_root),
            (SeasonStatus::Finalized, vec![5, 6], [4; 32])
        );
        assert_eq!(
            (s.outstanding, s.refund_base.len(), s.rules_hash),
            (0, 0, [0; 32])
        );
        assert_eq!(
            load_season(&program, &account).unwrap_err(),
            ChainError::NotInitialized.into()
        );
        // The current layout loads through both.
        let mut data = vec![0u8; SEASON_SPACE];
        store(&mut data, &season_fx(7, bump)).unwrap();
        let mut lamports = 0;
        let account = info(&key, &program, &mut lamports, &mut data);
        assert!(!load_season_compat(&program, &account).unwrap().1);
        assert_eq!(load_season(&program, &account).unwrap(), season_fx(7, bump));
        // Another magic, or another address: refused.
        let mut data = vec![0u8; SEASON_SPACE];
        let mut junk = season_fx(7, bump);
        junk.magic = *b"PSSEASN6";
        store(&mut data, &junk).unwrap();
        let mut lamports = 0;
        let account = info(&key, &program, &mut lamports, &mut data);
        assert_eq!(
            load_season_compat(&program, &account).unwrap_err(),
            ChainError::NotInitialized.into()
        );
        let elsewhere = Pubkey::new_unique();
        let mut data = vec![0u8; SEASON_SPACE];
        store(&mut data, &season_fx(7, bump)).unwrap();
        let mut lamports = 0;
        let account = info(&elsewhere, &program, &mut lamports, &mut data);
        assert_eq!(
            load_season_compat(&program, &account).unwrap_err(),
            ChainError::WrongPda.into()
        );
    }

    #[test]
    fn nation_heads_are_checked_like_nations() {
        let program = Pubkey::new_unique();
        let (pda, bump) = Pubkey::find_program_address(
            &[NATION_SEED, &7u64.to_le_bytes(), &1u16.to_le_bytes()],
            &program,
        );
        let mut d = nation(7, 1, bump);
        let mut l = 0;
        let real = info(&pda, &program, &mut l, &mut d);
        let head = load_nation_head_at(&program, &real).unwrap();
        assert_eq!((head.season_id, head.civ), (7, 1));
        let impostor = Pubkey::new_unique();
        let mut d = nation(7, 1, bump);
        let mut l = 0;
        let fake = info(&impostor, &program, &mut l, &mut d);
        assert_eq!(
            load_nation_head_at(&program, &fake).unwrap_err(),
            ChainError::WrongPda.into()
        );
        let other = Pubkey::new_unique();
        let mut d = nation(7, 1, bump);
        let mut l = 0;
        let foreign = info(&pda, &other, &mut l, &mut d);
        assert_eq!(
            load_nation_head_at(&program, &foreign).unwrap_err(),
            ChainError::MissingNation.into()
        );
    }

    #[test]
    fn world_chunk0_alone_is_checked() {
        let program = Pubkey::new_unique();
        let (pda, _) =
            Pubkey::find_program_address(&[WORLD_SEED, &7u64.to_le_bytes(), &[0]], &program);
        let meta = WorldMeta {
            season_id: 7,
            civs: 2,
            ..Default::default()
        };
        let chunk = |magic: [u8; 8]| {
            let mut d = vec![0u8; CHUNK];
            d[..8].copy_from_slice(&magic);
            meta.write_chunk0(&mut d).unwrap();
            d
        };
        let (mut d, mut l) = (chunk(WORLD_MAGIC), 0);
        assert_eq!(
            world_chunk0(&program, &info(&pda, &program, &mut l, &mut d)).unwrap(),
            meta
        );
        let (mut d, mut l) = (chunk(GENESIS_MAGIC), 0);
        assert_eq!(
            world_chunk0(&program, &info(&pda, &program, &mut l, &mut d)).unwrap_err(),
            ChainError::WrongWorld.into()
        );
        let (mut d, mut l) = (chunk(WORLD_MAGIC), 0);
        let other = Pubkey::new_unique();
        assert_eq!(
            world_chunk0(&program, &info(&pda, &other, &mut l, &mut d)).unwrap_err(),
            ChainError::WrongWorld.into()
        );
        let (mut d, mut l) = (chunk(WORLD_MAGIC), 0);
        let elsewhere = Pubkey::new_unique();
        assert_eq!(
            world_chunk0(&program, &info(&elsewhere, &program, &mut l, &mut d)).unwrap_err(),
            ChainError::WrongPda.into()
        );
    }

    #[test]
    fn vrf_accounts_are_checked() {
        use crate::randomness::{VRF_QUEUE_BASE, VRF_QUEUE_ER};
        let program = Pubkey::new_unique();
        let (identity, bump) = crate::randomness::program_identity(&program);
        let vrf = crate::randomness::VRF_PROGRAM_ID;
        let queue = VRF_QUEUE_ER;
        let system = system_program::id();
        let hashes = solana_program::sysvar::slot_hashes::id();
        let owner = Pubkey::default();
        let run = |identity: Pubkey,
                   queue: Pubkey,
                   vrf: Pubkey,
                   executable: bool,
                   writable: bool,
                   expected: Pubkey| {
            let (mut l, mut d) = ([0u64; 5], vec![vec![0u8; 0]; 5]);
            let [l0, l1, l2, l3, l4] = &mut l;
            let mut it = d.iter_mut();
            let (d0, d1, d2, d3, d4) = (
                it.next().unwrap(),
                it.next().unwrap(),
                it.next().unwrap(),
                it.next().unwrap(),
                it.next().unwrap(),
            );
            let i = AccountInfo::new(&identity, false, false, l0, d0, &owner, false);
            let q = AccountInfo::new(&queue, false, writable, l1, d1, &owner, false);
            let v = AccountInfo::new(&vrf, false, false, l2, d2, &owner, executable);
            let s = AccountInfo::new(&system, false, false, l3, d3, &owner, false);
            let h = AccountInfo::new(&hashes, false, false, l4, d4, &owner, false);
            check_vrf_accounts(&program, &i, &q, &v, &s, &h, &expected)
        };
        let wrong: ProgramError = ChainError::WrongOracle.into();
        // Each layer's queue is accepted only where it is expected (A20):
        // the other layer's queue is `WrongOracle`, both ways.
        assert_eq!(
            run(identity, VRF_QUEUE_ER, vrf, true, true, VRF_QUEUE_ER).unwrap(),
            bump
        );
        assert_eq!(
            run(identity, VRF_QUEUE_BASE, vrf, true, true, VRF_QUEUE_BASE).unwrap(),
            bump
        );
        assert_eq!(
            run(identity, VRF_QUEUE_BASE, vrf, true, true, VRF_QUEUE_ER).unwrap_err(),
            wrong
        );
        assert_eq!(
            run(identity, VRF_QUEUE_ER, vrf, true, true, VRF_QUEUE_BASE).unwrap_err(),
            wrong
        );
        let er = |identity, queue, vrf, executable, writable| {
            run(identity, queue, vrf, executable, writable, VRF_QUEUE_ER)
        };
        assert_eq!(
            er(Pubkey::new_unique(), queue, vrf, true, true).unwrap_err(),
            wrong
        );
        assert_eq!(
            er(identity, Pubkey::new_unique(), vrf, true, true).unwrap_err(),
            wrong
        );
        assert_eq!(
            er(identity, queue, Pubkey::new_unique(), true, true).unwrap_err(),
            wrong
        );
        assert_eq!(er(identity, queue, vrf, false, true).unwrap_err(), wrong);
        assert_eq!(er(identity, queue, vrf, true, false).unwrap_err(), wrong);
    }

    #[test]
    fn the_vault_is_the_seasons_token_account() {
        let program = Pubkey::new_unique();
        let season_key = Pubkey::new_unique();
        let s = season_fx(7, 0);
        let (vault, _) = Pubkey::find_program_address(&[VAULT_SEED, &7u64.to_le_bytes()], &program);
        let account = |mint: [u8; 32], owner: Pubkey| {
            let mut d = vec![0u8; token::TOKEN_ACCOUNT_LEN];
            d[..32].copy_from_slice(&mint);
            d[32..64].copy_from_slice(owner.as_ref());
            d[64..72].copy_from_slice(&42u64.to_le_bytes());
            d
        };
        let t = token::TOKEN_PROGRAM_ID;
        let (mut d, mut l) = (account(s.usdc_mint, season_key), 0);
        assert_eq!(
            vault_amount(&program, &s, &season_key, &info(&vault, &t, &mut l, &mut d)).unwrap(),
            42
        );
        let wrong: ProgramError = ChainError::WrongTokenAccount.into();
        let (mut d, mut l) = (account([9; 32], season_key), 0);
        assert_eq!(
            vault_amount(&program, &s, &season_key, &info(&vault, &t, &mut l, &mut d)).unwrap_err(),
            wrong
        );
        let (mut d, mut l) = (account(s.usdc_mint, Pubkey::new_unique()), 0);
        assert_eq!(
            vault_amount(&program, &s, &season_key, &info(&vault, &t, &mut l, &mut d)).unwrap_err(),
            wrong
        );
        let (mut d, mut l) = (account(s.usdc_mint, season_key), 0);
        let elsewhere = Pubkey::new_unique();
        assert_eq!(
            vault_amount(
                &program,
                &s,
                &season_key,
                &info(&elsewhere, &t, &mut l, &mut d)
            )
            .unwrap_err(),
            ChainError::WrongPda.into()
        );
    }

    #[test]
    fn outsiders_take_over_only_when_the_stage_is_overdue() {
        let s = season_fx(7, 0);
        let (operator, outsider) = (Pubkey::new_from_array(s.crank), Pubkey::new_unique());
        let owner = Pubkey::default();
        let (mut l1, mut l2) = (0u64, 0u64);
        let (mut d1, mut d2) = ([0u8; 0], [0u8; 0]);
        let op = AccountInfo::new(&operator, true, false, &mut l1, &mut d1, &owner, false);
        let out = AccountInfo::new(&outsider, true, false, &mut l2, &mut d2, &owner, false);
        let at = s.stage_at + TAKEOVER_SECONDS;
        assert!(require_operator_or_overdue(&s, &op, 0).is_ok());
        assert_eq!(
            require_operator_or_overdue(&s, &out, at - 1).unwrap_err(),
            ChainError::Unauthorized.into()
        );
        assert!(require_operator_or_overdue(&s, &out, at).is_ok());
    }

    #[test]
    fn the_host_clock_is_settable() {
        set_host_clock(1_234);
        assert_eq!(now().unwrap(), 1_234);
        set_host_clock(None);
        // Off chain there is no Clock sysvar.
        assert!(now().is_err());
    }
}
