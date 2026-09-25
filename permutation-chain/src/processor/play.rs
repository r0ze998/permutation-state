//! On the Ephemeral Rollup, during play: sealed orders (commit, close,
//! reveal), governance submissions, publishing the tick input, and resolving
//! the tick.

use borsh::BorshDeserialize;
use permutation_rules::gov::{GovAction, GovEntry, Role, NOBODY};
use permutation_rules::orders::{
    check_structure, order_commitment, role_allows_static, Order, OrderBatch,
};
use permutation_rules::rng::{tick_vrf, Salt};
use permutation_rules::state::WorldState;
use permutation_rules::tick::{run_phase, TickInput, PHASE_COUNT};
use permutation_rules::Ruleset;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint::ProgramResult,
    hash::hashv,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar::Sysvar,
};

use super::accounts::*;
use crate::error::ChainError;
use crate::state::*;

/// Refresh a nation account for the open tick from the world: office
/// holders, their keys and budgets; clear the batches and the inbox.
pub(super) fn open_nation(n: &mut NationAccount, state: &WorldState, rules: &Ruleset) {
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
    n.revealing = false;
    n.reveal_deadline = 0;
    n.committed = [u16::MAX; 4];
    n.commits = [[0; 32]; 4];
    n.salts = [[0; 32]; 4];
    n.inbox.clear();
}

/// Seal one office's orders for the open tick (commit–reveal): only the
/// commitment is stored, so nobody can read or react to them before every
/// office is locked in.
pub(super) fn commit_orders(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    role: Role,
    tick: u16,
    commitment: [u8; 32],
) -> ProgramResult {
    let it = &mut accounts.iter();
    let who = next_account_info(it)?;
    let nation_ai = next_account_info(it)?;
    signer(who)?;
    let mut n = load_nation_at(program_id, nation_ai)?;
    let i = role.index();
    // Only the office holder's session key. A vacant office takes no batch
    // from anyone, the operator included: the rules' caretaker fills it
    // (permutation_rules::gov::caretaker).
    if n.officers[i] == NOBODY || who.key.as_ref() != n.keys[i] {
        return Err(ChainError::Unauthorized.into());
    }
    if tick != n.open_tick {
        return Err(ChainError::WrongTick.into());
    }
    if n.frozen {
        return Err(ChainError::TickFrozen.into());
    }
    if n.revealing {
        return Err(ChainError::WrongPhase.into());
    }
    if commitment == [0; 32] {
        return Err(ChainError::InvalidParams.into());
    }
    n.commits[i] = commitment;
    n.committed[i] = tick;
    store(&mut nation_ai.try_borrow_mut_data()?, &n)
}

/// A revealed batch's fields besides its office and tick.
pub(super) struct OrderBatchParts {
    pub decision_digest: [u8; 32],
    pub orders: Vec<Order>,
    pub adopt: Vec<u32>,
}

/// Reveal one office's sealed orders in the reveal window.
pub(super) fn reveal_orders(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    role: Role,
    tick: u16,
    parts: OrderBatchParts,
    salt: [u8; 32],
) -> ProgramResult {
    let it = &mut accounts.iter();
    let who = next_account_info(it)?;
    let nation_ai = next_account_info(it)?;
    signer(who)?;
    let mut n = load_nation_at(program_id, nation_ai)?;
    let i = role.index();
    if tick != n.open_tick || n.committed[i] != tick {
        return Err(ChainError::WrongTick.into());
    }
    if n.frozen {
        return Err(ChainError::TickFrozen.into());
    }
    if !n.revealing {
        return Err(ChainError::WrongPhase.into());
    }
    if Clock::get()?.unix_timestamp > n.reveal_deadline {
        return Err(ChainError::TickFrozen.into()); // the reveal window is over
    }
    let batch = OrderBatch {
        civ: n.civ,
        tick,
        role,
        member: n.officers[i],
        decision_digest: parts.decision_digest,
        orders: parts.orders,
        adopt: parts.adopt,
    };
    if order_commitment(&batch, &salt) != n.commits[i] {
        return Err(ChainError::CommitMismatch.into());
    }
    // Officers seal a rationale (V5 D17).
    if batch.decision_digest == [0; 32] {
        return Err(ChainError::Rules.into());
    }
    if !batch.orders.iter().all(|o| role_allows_static(role, o)) {
        return Err(ChainError::WrongOffice.into());
    }
    let rules = rules_for(n.preset, n.market)?;
    let cost = check_structure(&rules, tick, &batch.orders).map_err(|_| ChainError::Rules)?;
    if cost > n.spendable[i] || batch.adopt.len() > rules.max_open_proposals as usize {
        return Err(ChainError::OverBudget.into());
    }
    n.batches[i] = Some(batch);
    n.submitted[i] = tick;
    n.salts[i] = salt;
    store(&mut nation_ai.try_borrow_mut_data()?, &n)
}

/// Close the open tick's commitments after its deadline and open the reveal
/// window. Governance closes with them, so nobody acts on revealed orders.
pub(super) fn close_commits(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let chunk0 = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let world = world_chunks(program_id, accounts, world_season_id(chunk0)?)?;
    let mut meta = world.meta()?;
    if meta.finished || meta.frozen || meta.revealing {
        return Err(ChainError::WrongPhase.into());
    }
    let clock = Clock::get()?;
    if clock.unix_timestamp < meta.deadline {
        return Err(ChainError::TooEarly.into());
    }
    let it = &mut accounts[WORLD_CHUNKS..].iter();
    let mut commits: Vec<(u16, u8, u32, [u8; 32])> = Vec::new();
    let mut tick = u16::MAX;
    for civ in 0..meta.civs as u16 {
        let ai = next_account_info(it)?;
        let mut n = load_nation(program_id, ai, meta.season_id, civ)?;
        tick = n.open_tick;
        for role in Role::ALL {
            let i = role.index();
            if n.committed[i] == n.open_tick {
                commits.push((civ, i as u8, n.officers[i], n.commits[i]));
            }
        }
        n.revealing = true;
        n.reveal_deadline = clock.unix_timestamp + reveal_seconds(meta.tick_seconds);
        store(&mut ai.try_borrow_mut_data()?, &n)?;
    }
    meta.revealing = true;
    meta.deadline = clock.unix_timestamp + reveal_seconds(meta.tick_seconds);
    world.set_meta(&meta)?;
    let record = borsh::to_vec(&commits).map_err(|_| ChainError::Rules)?;
    solana_program::log::sol_log_data(&[b"PS_COMMITS", &tick.to_le_bytes(), &record]);
    Ok(())
}

pub(super) fn submit_gov(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    member: u32,
    action: GovAction,
) -> ProgramResult {
    let it = &mut accounts.iter();
    let who = next_account_info(it)?;
    let nation_ai = next_account_info(it)?;
    signer(who)?;
    let mut n = load_nation_at(program_id, nation_ai)?;
    if n.open_tick == u16::MAX {
        return Err(ChainError::WrongStatus.into());
    }
    if n.frozen || n.revealing {
        return Err(ChainError::TickFrozen.into());
    }
    let signer_key = who.key.to_bytes();
    if n.inbox.iter().filter(|e| e.signer == signer_key).count() >= MAX_GOV_PER_SIGNER {
        return Err(ChainError::InboxFull.into());
    }
    n.inbox.push(GovEntry {
        member,
        signer: signer_key,
        action,
    });
    // Keep room for every office's reveal still to come: a flood of
    // governance actions must not crowd out a nation's orders.
    let unrevealed = (0..4).filter(|i| n.submitted[*i] != n.open_tick).count();
    let len = borsh::to_vec(&n).map_err(|_| ChainError::Rules)?.len();
    if len + unrevealed * REVEAL_ROOM > NATION_SPACE {
        return Err(ChainError::InboxFull.into());
    }
    store(&mut nation_ai.try_borrow_mut_data()?, &n)
}

/// The open tick's input as it stands in the nation accounts, with `vrf`
/// as its randomness.
pub(super) struct Pending<'a, 'info> {
    /// Revealed batches (in civ, then office order) and governance inboxes.
    pub input: TickInput,
    pub tick: u16,
    /// Commitments of the open tick, and how many of them were revealed.
    pub committed: usize,
    pub revealed: usize,
    /// Each revealed batch's salt, in the same order (the tick randomness).
    pub salts: Vec<Salt>,
    pub ais: Vec<&'a AccountInfo<'info>>,
}

pub(super) fn pending_input<'a, 'info>(
    program_id: &Pubkey,
    nations: &'a [AccountInfo<'info>],
    meta: &WorldMeta,
    vrf: [u8; 32],
) -> Result<Pending<'a, 'info>, ProgramError> {
    let it = &mut nations.iter();
    let mut p = Pending {
        input: TickInput {
            vrf,
            batches: Vec::new(),
            gov: Vec::new(),
            deposits: Vec::new(),
        },
        tick: u16::MAX,
        committed: 0,
        revealed: 0,
        salts: Vec::new(),
        ais: Vec::with_capacity(meta.civs as usize),
    };
    for civ in 0..meta.civs as u16 {
        let ai = next_account_info(it)?;
        let n = load_nation(program_id, ai, meta.season_id, civ)?;
        if p.tick == u16::MAX {
            p.tick = n.open_tick;
        }
        for role in Role::ALL {
            let i = role.index();
            if n.committed[i] == n.open_tick {
                p.committed += 1;
            }
            if let Some(b) = n.batches[i].clone().filter(|b| b.tick == n.open_tick) {
                p.input.batches.push(b);
                p.salts.push((civ, i as u8, n.salts[i]));
                p.revealed += 1;
            }
        }
        p.input.gov.extend(n.inbox);
        p.ais.push(ai);
    }
    Ok(p)
}

pub(super) fn log_tick_input(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    chunk: u16,
) -> ProgramResult {
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
        if !meta.revealing {
            return Err(ChainError::WrongPhase.into());
        }
        let p = pending_input(program_id, nations, &meta, [0; 32])?;
        let clock = Clock::get()?;
        if clock.unix_timestamp < meta.deadline && p.revealed < p.committed {
            return Err(ChainError::TooEarly.into());
        }
        // Tick randomness (§0.2): the world root and every revealed salt.
        // The salts were sealed before the commitments closed and only their
        // officers knew them, so nobody (the crank choosing this slot
        // included) can pick the outcome.
        let body = world.body(&WORLD_MAGIC)?;
        let root = hashv(&[&body]).to_bytes();
        drop(body);
        meta.vrf = tick_vrf(&root, &p.salts);
        meta.frozen = true;
        meta.input_logged = 0;
        for ai in p.ais {
            let mut n: NationAccount = load(&ai.try_borrow_data()?)?;
            n.frozen = true;
            store(&mut ai.try_borrow_mut_data()?, &n)?;
        }
    }
    let Pending {
        input, tick, salts, ..
    } = pending_input(program_id, nations, &meta, meta.vrf)?;
    // Every chunk-0 call logs the salts with the world root it sees, so the
    // record is in whichever transaction the index keeps (the verifier takes
    // the one whose root is the tick's pre-state root).
    if chunk == 0 {
        let body = world.body(&WORLD_MAGIC)?;
        let root = hashv(&[&body]).to_bytes();
        drop(body);
        let bytes = borsh::to_vec(&salts).map_err(|_| ChainError::Rules)?;
        solana_program::log::sol_log_data(&[b"PS_SALTS", &tick.to_le_bytes(), &root, &bytes]);
    }
    let bytes = borsh::to_vec(&input).map_err(|_| ChainError::Rules)?;
    let total = bytes.len().div_ceil(INPUT_CHUNK).max(1) as u16;
    if chunk >= total || chunk > meta.input_logged {
        return Err(ChainError::InvalidParams.into());
    }
    meta.input_chunks = total;
    meta.input_logged = meta.input_logged.max(chunk + 1);
    world.set_meta(&meta)?;
    let hash = hashv(&[&bytes]).to_bytes();
    let part =
        &bytes[chunk as usize * INPUT_CHUNK..bytes.len().min((chunk as usize + 1) * INPUT_CHUNK)];
    solana_program::log::sol_log_data(&[
        b"PS_INPUT",
        &tick.to_le_bytes(),
        &chunk.to_le_bytes(),
        &total.to_le_bytes(),
        &hash,
        part,
    ]);
    Ok(())
}

/// `cu-trace` builds log the compute units left at `what`.
macro_rules! cu {
    ($what:expr) => {
        #[cfg(feature = "cu-trace")]
        {
            msg!($what);
            solana_program::log::sol_log_compute_units();
        }
    };
}

pub(super) fn resolve_tick(program_id: &Pubkey, accounts: &[AccountInfo], to: u8) -> ProgramResult {
    cu!("cu start");
    let chunk0 = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    let world = world_chunks(program_id, accounts, world_season_id(chunk0)?)?;
    let mut meta = world.meta()?;
    cu!("cu chunks");
    if meta.finished {
        return Err(ChainError::WrongStatus.into());
    }
    // Data availability: a tick resolves only on an input the chain published.
    if !meta.frozen || meta.input_logged < meta.input_chunks || meta.input_chunks == 0 {
        return Err(ChainError::InputNotPublished.into());
    }
    let body = world.body(&WORLD_MAGIC)?;
    cu!("cu body");
    let pre_root = hashv(&[&body]).to_bytes();
    cu!("cu hash");
    let mut state = WorldState::try_from_slice(&body).map_err(|_| ChainError::WrongWorld)?;
    drop(body);
    cu!("cu decode");
    let rules = rules_for(meta.preset, meta.market)?;
    // Submissions are refused while frozen, so this is the input PS_INPUT published.
    let Pending {
        input,
        ais: nation_ais,
        ..
    } = pending_input(program_id, &accounts[WORLD_CHUNKS..], &meta, meta.vrf)?;
    cu!("cu input");
    let tick = state.tick;
    while state.phase_cursor < to.min(PHASE_COUNT) {
        let p = state.phase_cursor;
        run_phase(&mut state, &rules, &input, p).map_err(|_| ChainError::Rules)?;
        cu!("cu phase");
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
        meta.revealing = false;
    }
    cu!("cu nations");
    let root = world.write_world(&state)?;
    world.set_meta(&meta)?;
    cu!("cu write");
    // Tick record for the replay verifier: the step from the previous root
    // to this one, and the hash of the input (published in full as PS_INPUT
    // before the tick could resolve). A split tick logs one record per part.
    let record = borsh::to_vec(&input).map_err(|_| ChainError::Rules)?;
    let input_hash = hashv(&[&record]).to_bytes();
    solana_program::log::sol_log_data(&[
        b"PS_TICK",
        &tick.to_le_bytes(),
        &[to],
        &pre_root,
        &root,
        &input_hash,
    ]);
    msg!(
        "PS tick {} {}",
        tick,
        if completed { "resolved" } else { "partial" }
    );
    Ok(())
}
