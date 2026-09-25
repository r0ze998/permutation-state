//! On the Ephemeral Rollup, during play: submissions, publishing the tick
//! input, and resolving the tick.

use borsh::BorshDeserialize;
use permutation_rules::gov::{GovAction, GovEntry, Role, NOBODY};
use permutation_rules::orders::{check_structure, role_allows_static, Order, OrderBatch};
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
    n.inbox.clear();
}

#[allow(clippy::too_many_arguments)]
pub(super) fn submit_orders(
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
    let mut n = load_nation_at(program_id, nation_ai)?;
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
    n.batches[i] = Some(OrderBatch {
        civ: n.civ,
        tick,
        role,
        member,
        decision_digest,
        orders,
        adopt,
    });
    n.submitted[i] = tick;
    store(&mut nation_ai.try_borrow_mut_data()?, &n)?;
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
    if n.frozen {
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
    // Fails with AccountDataTooSmall when the inbox is full for this tick.
    store(&mut nation_ai.try_borrow_mut_data()?, &n)
}

/// The open tick's input as it stands in the nation accounts (batches for
/// the open tick, governance inboxes), with `vrf` as its randomness.
pub(super) fn pending_input<'a, 'info>(
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
    Ok((
        TickInput {
            vrf,
            batches,
            gov,
            deposits: Vec::new(),
        },
        tick,
        done,
        ais,
    ))
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
        meta.vrf = hashv(&[
            b"PS/tick-vrf/v2",
            &root,
            &clock.slot.to_le_bytes(),
            &clock.unix_timestamp.to_le_bytes(),
        ])
        .to_bytes();
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

pub(super) fn resolve_tick(program_id: &Pubkey, accounts: &[AccountInfo], to: u8) -> ProgramResult {
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
    let (input, _, _, nation_ais) =
        pending_input(program_id, &accounts[WORLD_CHUNKS..], &meta, meta.vrf)?;
    let tick = state.tick;
    while state.phase_cursor < to.min(PHASE_COUNT) {
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
