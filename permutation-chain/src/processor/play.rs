//! On the Ephemeral Rollup, during play: sealed orders (commit, close,
//! reveal), governance submissions, freezing the tick and drawing its
//! randomness from the MagicBlock VRF, publishing the tick input, and
//! resolving the tick.
//!
//! A tick: CommitOrders / SubmitGov until the deadline → CloseCommits →
//! RevealOrders in the reveal window → FreezeTick → RetryTickRandomness (the
//! VRF request) → ConsumeTickRandomness (the oracle's callback) →
//! LogTickInput for every chunk → ResolveTick in parts.

use borsh::{BorshDeserialize, BorshSerialize};
use permutation_rules::gov::{GovAction, GovEntry, Role, NOBODY};
use permutation_rules::orders::{
    check_batch_size, check_structure, order_commitment, role_allows_static, Order, OrderBatch,
};
use permutation_rules::rng::Salt;
use permutation_rules::state::WorldState;
use permutation_rules::tick::{run_phase, run_phase_degraded, TickInput, PHASE_COUNT};
use permutation_rules::{RulesError, Ruleset};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    hash::hashv,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};

use super::accounts::*;
use crate::error::ChainError;
use crate::randomness::{self, RAND_FALLBACK, RAND_NONE, RAND_PENDING, RAND_VRF};
use crate::state::*;

/// Runs `f` and gives back every heap allocation it made (the bump heap
/// never frees): a loop over the nation accounts decodes one at a time in
/// the same memory. Off SBF it just runs `f`.
///
/// # Safety
/// The result owns no heap (`Copy + 'static`), and `f` captures only shared
/// references and account infos; `heap::release` panics if the pointer went
/// below the mark.
unsafe fn scoped<R: Copy + 'static>(f: impl FnOnce() -> R) -> R {
    let mark = crate::heap::mark();
    let r = f();
    crate::heap::release(mark);
    r
}

/// `v` borsh-encoded into a buffer of exactly its length (a growing `Vec`
/// leaves every outgrown buffer behind on the bump heap).
fn to_vec_exact<T: BorshSerialize>(v: &T) -> Result<Vec<u8>, ProgramError> {
    let len = borsh::object_length(v).map_err(|_| ChainError::Rules)?;
    let mut out = Vec::with_capacity(len);
    v.serialize(&mut out).map_err(|_| ChainError::Rules)?;
    Ok(out)
}

/// Writes `head` over the fixed front of its nation account in place: the
/// batches, the inbox and the roll after it are left as they are (no
/// decode, no heap). Only for fields of the head.
fn store_head(info: &AccountInfo, head: &NationHead) -> ProgramResult {
    let mut data = info.try_borrow_mut_data()?;
    let mut front = data
        .get_mut(..NATION_HEAD_LEN)
        .ok_or(ChainError::MissingNation)?;
    head.serialize(&mut front)
        .map_err(|_| ChainError::MissingNation.into())
}

/// The fixed front of the nation account of `civ` in `season_id`.
fn nation_head(
    program_id: &Pubkey,
    info: &AccountInfo,
    season_id: u64,
    civ: u16,
) -> Result<NationHead, ProgramError> {
    let n = load_nation_head_at(program_id, info)?;
    if n.season_id != season_id || n.civ != civ {
        return Err(ChainError::MissingNation.into());
    }
    Ok(n)
}

/// Refresh a nation account for the open tick from the world: office
/// holders, their keys and budgets; clear the batches and the inbox; the
/// tick's `deadline`.
pub(super) fn open_nation(
    n: &mut NationAccount,
    state: &WorldState,
    rules: &Ruleset,
    deadline: i64,
) {
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
    n.clear_tick();
    n.deadline = deadline;
}

/// `StartClock`: once the accounts are on the layer that plays tick 0, the
/// crank starts its clock: the deadline becomes `now + tick_seconds` where
/// that is earlier than the one `OpenGovernment` set (with
/// `TICK0_GRACE_SECONDS`). Tick 0 only, before its commitments close.
pub(super) fn start_clock(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let crank = accounts.first().ok_or(ProgramError::NotEnoughAccountKeys)?;
    signer(crank)?;
    let world = world_at(program_id, &accounts[1..])?;
    if !world.is_world() {
        return Err(ChainError::WrongStatus.into());
    }
    let mut meta = world.meta()?;
    if meta.finished || meta.frozen || meta.revealing {
        return Err(ChainError::WrongPhase.into());
    }
    let nations = accounts
        .get(1 + WORLD_CHUNKS..1 + WORLD_CHUNKS + meta.civs as usize)
        .ok_or(ProgramError::NotEnoughAccountKeys)?;
    require_crank(program_id, &nations[0], meta.season_id, crank)?;
    let deadline = now()? + meta.tick_seconds as i64;
    // Every nation on tick 0 before any write.
    for (civ, info) in nations.iter().enumerate() {
        if nation_head(program_id, info, meta.season_id, civ as u16)?.open_tick != 0 {
            return Err(ChainError::WrongTick.into());
        }
    }
    for (civ, info) in nations.iter().enumerate() {
        let mut n = nation_head(program_id, info, meta.season_id, civ as u16)?;
        if deadline < n.deadline {
            n.deadline = deadline;
            store_head(info, &n)?;
        }
    }
    if deadline < meta.deadline {
        meta.deadline = deadline;
        world.set_meta(&meta)?;
    }
    msg!("PS clock: tick 0 closes at {}", meta.deadline);
    Ok(())
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
    let nation_info = next_account_info(it)?;
    signer(who)?;
    let mut n = load_nation_at(program_id, nation_info)?;
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
    // The commitments close at the deadline, whether or not anyone has
    // sent CloseCommits yet: a late seal could be chosen knowing the others.
    if now()? >= n.deadline {
        return Err(ChainError::WrongPhase.into());
    }
    if commitment == [0; 32] {
        return Err(ChainError::InvalidParams.into());
    }
    n.commits[i] = commitment;
    n.committed[i] = tick;
    store(&mut nation_info.try_borrow_mut_data()?, &n)
}

/// A revealed batch's fields besides its office and tick.
pub(super) struct OrderBatchParts {
    pub decision_digest: [u8; 32],
    pub orders: Vec<Order>,
    pub adopt: Vec<u32>,
}

/// The batch caps that need no rules or world: at most
/// `orders::MAX_BATCH_ORDERS` orders and adopted proposals, and at most
/// `MAX_REVEAL_BYTES` of orders (what one transaction can carry, and what
/// `REVEAL_ROOM` keeps free in the nation account).
fn check_reveal_size(orders: &[Order], adopt: &[u32]) -> Result<(), ChainError> {
    check_batch_size(orders, adopt).map_err(|_| ChainError::OverBudget)?;
    let mut bytes = 0usize;
    for o in orders {
        bytes += borsh::object_length(o).map_err(|_| ChainError::OverBudget)?;
        if bytes > MAX_REVEAL_BYTES {
            return Err(ChainError::OverBudget);
        }
    }
    Ok(())
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
    let nation_info = next_account_info(it)?;
    signer(who)?;
    let mut n = load_nation_at(program_id, nation_info)?;
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
    if now()? > n.deadline {
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
    check_reveal_size(&batch.orders, &batch.adopt)?;
    if !batch.orders.iter().all(|o| role_allows_static(role, o)) {
        return Err(ChainError::WrongOffice.into());
    }
    let rules = rules_for(n.preset, n.market)?;
    let cost = check_structure(&rules, tick, &batch.orders).map_err(|e| match e {
        RulesError::TooManyOrders => ChainError::OverBudget,
        _ => ChainError::Rules,
    })?;
    if cost > n.spendable[i] || batch.adopt.len() > rules.max_open_proposals as usize {
        return Err(ChainError::OverBudget.into());
    }
    n.batches[i] = Some(batch);
    n.submitted[i] = tick;
    n.salts[i] = salt;
    store(&mut nation_info.try_borrow_mut_data()?, &n)
}

/// Close the open tick's commitments after its deadline and open the reveal
/// window. Governance closes with them, so nobody acts on revealed orders.
pub(super) fn close_commits(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let world = world_at(program_id, accounts)?;
    let mut meta = world.meta()?;
    let civs = meta.civs as usize;
    require_alone(program_id, accounts.get(WORLD_CHUNKS + civs))?;
    if !world.is_world() {
        return Err(ChainError::WrongStatus.into());
    }
    if meta.finished || meta.frozen || meta.revealing {
        return Err(ChainError::WrongPhase.into());
    }
    let now = now()?;
    if now < meta.deadline {
        return Err(ChainError::TooEarly.into());
    }
    let nations = accounts
        .get(WORLD_CHUNKS..WORLD_CHUNKS + civs)
        .ok_or(ProgramError::NotEnoughAccountKeys)?;
    // The government must be open, every nation on the same tick, before
    // anything is written.
    let mut commits: Vec<(u16, u8, u32, [u8; 32])> = Vec::with_capacity(4 * civs);
    let mut tick = NO_TICK;
    for (civ, info) in nations.iter().enumerate() {
        let n = nation_head(program_id, info, meta.season_id, civ as u16)?;
        if n.open_tick == NO_TICK {
            return Err(ChainError::WrongStatus.into());
        }
        if civ > 0 && n.open_tick != tick {
            return Err(ChainError::WrongTick.into());
        }
        tick = n.open_tick;
        for role in Role::ALL {
            let i = role.index();
            if n.committed[i] == n.open_tick {
                commits.push((civ as u16, i as u8, n.officers[i], n.commits[i]));
            }
        }
    }
    let until = now + reveal_seconds(meta.tick_seconds);
    for (civ, info) in nations.iter().enumerate() {
        let mut n = nation_head(program_id, info, meta.season_id, civ as u16)?;
        n.revealing = true;
        n.deadline = until;
        store_head(info, &n)?;
    }
    meta.revealing = true;
    meta.deadline = until;
    world.set_meta(&meta)?;
    let record = to_vec_exact(&commits)?;
    solana_program::log::sol_log_data(&[b"PS_COMMITS", &tick.to_le_bytes(), &record]);
    Ok(())
}

/// Queue a governance action for the open tick: the signer is the member's
/// seated key (the nation's roll), within the member's quota of slots.
pub(super) fn submit_gov(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    member: u32,
    action: GovAction,
) -> ProgramResult {
    let it = &mut accounts.iter();
    let who = next_account_info(it)?;
    let nation_info = next_account_info(it)?;
    signer(who)?;
    let mut n = load_nation_at(program_id, nation_info)?;
    if n.open_tick == NO_TICK {
        return Err(ChainError::WrongStatus.into());
    }
    let rules = rules_for(n.preset, n.market)?;
    // After the last tick the nations stay open on tick `ticks_per_season`
    // until they are undelegated: nothing may be queued for it.
    if n.open_tick >= rules.ticks_per_season {
        return Err(ChainError::WrongStatus.into());
    }
    if n.frozen || n.revealing {
        return Err(ChainError::TickFrozen.into());
    }
    if now()? >= n.deadline {
        return Err(ChainError::TickFrozen.into());
    }
    let signer_key = who.key.to_bytes();
    let k8 = key8(&signer_key);
    if !n.roll.iter().any(|s| s.member == member && s.key8 == k8) {
        return Err(ChainError::Unauthorized.into());
    }
    let slots = gov_slots(&action).ok_or(ChainError::InvalidParams)?;
    if let GovAction::Propose { orders, .. } = &action {
        if orders.is_empty() || orders.len() > rules.max_proposal_orders as usize {
            return Err(ChainError::InvalidParams.into());
        }
    }
    let (mut count, mut used) = (0usize, 0u32);
    for e in n.inbox.iter().filter(|e| e.member == member) {
        count += 1;
        used += gov_slots(&e.action).unwrap_or(u16::MAX) as u32;
    }
    if count >= MAX_GOV_PER_SIGNER || used + slots as u32 > n.gov_quota as u32 {
        return Err(ChainError::InboxFull.into());
    }
    n.inbox.push(GovEntry {
        member,
        signer: signer_key,
        action,
    });
    // Keep room for every office's reveal still to come: governance must
    // never crowd out a nation's orders (the quotas keep this true; this is
    // the backstop).
    let unrevealed = (0..4).filter(|i| n.submitted[*i] != n.open_tick).count();
    let len = borsh::object_length(&n).map_err(|_| ChainError::Rules)?;
    if len + unrevealed * REVEAL_ROOM > NATION_SPACE {
        return Err(ChainError::InboxFull.into());
    }
    store(&mut nation_info.try_borrow_mut_data()?, &n)
}

/// The open tick's input as it stands in the nation accounts, with `vrf`
/// as its randomness.
pub(super) struct Pending {
    /// Revealed batches (in civ, then office order) and governance inboxes.
    pub input: TickInput,
    pub tick: u16,
    /// Each revealed batch's salt, in the same order: `randomness::salts_hash`
    /// commits to them.
    pub salts: Vec<Salt>,
}

/// One nation account read by `pending_input`'s first pass: its head, its
/// batches (decoded once, then moved into the input), and where its inbox
/// entries start and how many there are.
struct NationRead {
    head: NationHead,
    batches: [Option<OrderBatch>; 4],
    at: usize,
    count: u32,
}

/// The tick input from the nation accounts in two passes, so every vector
/// of it is allocated once at its final size (the bump heap never frees
/// an outgrown buffer). Only the head, the batches and the inbox are read.
pub(super) fn pending_input(
    program_id: &Pubkey,
    nations: &[AccountInfo],
    meta: &WorldMeta,
    vrf: [u8; 32],
) -> Result<Pending, ProgramError> {
    let civs = meta.civs as usize;
    let nations = nations
        .get(..civs)
        .ok_or(ProgramError::NotEnoughAccountKeys)?;
    let mut reads: Vec<NationRead> = Vec::with_capacity(civs);
    for (civ, info) in nations.iter().enumerate() {
        let head = nation_head(program_id, info, meta.season_id, civ as u16)?;
        let data = info.try_borrow_data()?;
        let mut r: &[u8] = data
            .get(NATION_HEAD_LEN..)
            .ok_or(ChainError::NotInitialized)?;
        let batches = <[Option<OrderBatch>; 4]>::deserialize(&mut r)
            .map_err(|_| ChainError::NotInitialized)?;
        let count = u32::deserialize(&mut r).map_err(|_| ChainError::NotInitialized)?;
        let at = data.len() - r.len();
        reads.push(NationRead {
            head,
            batches,
            at,
            count,
        });
    }
    let open = |r: &NationRead| {
        r.batches
            .iter()
            .filter(|b| b.as_ref().is_some_and(|b| b.tick == r.head.open_tick))
            .count()
    };
    let revealed: usize = reads.iter().map(open).sum();
    let entries: usize = reads.iter().map(|r| r.count as usize).sum();
    let mut p = Pending {
        input: TickInput {
            vrf,
            batches: Vec::with_capacity(revealed),
            gov: Vec::with_capacity(entries),
            deposits: Vec::new(),
        },
        tick: reads.first().map_or(NO_TICK, |r| r.head.open_tick),
        salts: Vec::with_capacity(revealed),
    };
    for (civ, (r, info)) in reads.into_iter().zip(nations).enumerate() {
        let NationRead {
            head,
            batches,
            at,
            count,
        } = r;
        for (i, b) in batches.into_iter().enumerate() {
            if let Some(b) = b.filter(|b| b.tick == head.open_tick) {
                p.input.batches.push(b);
                p.salts.push((civ as u16, i as u8, head.salts[i]));
            }
        }
        let data = info.try_borrow_data()?;
        let mut r: &[u8] = data.get(at..).ok_or(ChainError::NotInitialized)?;
        for _ in 0..count {
            p.input
                .gov
                .push(GovEntry::deserialize(&mut r).map_err(|_| ChainError::NotInitialized)?);
        }
    }
    Ok(p)
}

/// The VRF queue of the play mode the chain recorded (A20): a season whose
/// world chunk 0 was delegated (`Season::delegated`, set by `Delegate`
/// before the delegation; chunk 0 goes last) plays on the ER and draws from
/// `VRF_QUEUE_ER`; a season played on the base layer from `VRF_QUEUE_BASE`.
/// Never either: a request in the other layer's queue is never answered.
#[inline(never)]
fn play_queue(
    program_id: &Pubkey,
    season_info: &AccountInfo,
    season_id: u64,
) -> Result<Pubkey, ProgramError> {
    let season = load_season(program_id, season_info)?;
    if season.season_id != season_id {
        return Err(ChainError::WrongWorld.into());
    }
    Ok(if season.delegated & 1 != 0 {
        randomness::VRF_QUEUE_ER
    } else {
        randomness::VRF_QUEUE_BASE
    })
}

/// The accounts FreezeTick and RetryTickRandomness share, in order.
struct VrfAccounts<'a, 'info> {
    payer: &'a AccountInfo<'info>,
    chunk0: &'a AccountInfo<'info>,
    identity: &'a AccountInfo<'info>,
    queue: &'a AccountInfo<'info>,
    vrf: &'a AccountInfo<'info>,
    system: &'a AccountInfo<'info>,
    slot_hashes: &'a AccountInfo<'info>,
}

impl<'a, 'info> VrfAccounts<'a, 'info> {
    fn new(accounts: &'a [AccountInfo<'info>]) -> Result<Self, ProgramError> {
        let it = &mut accounts.iter();
        Ok(VrfAccounts {
            payer: next_account_info(it)?,
            chunk0: next_account_info(it)?,
            identity: next_account_info(it)?,
            queue: next_account_info(it)?,
            vrf: next_account_info(it)?,
            system: next_account_info(it)?,
            slot_hashes: next_account_info(it)?,
        })
    }

    /// `check_vrf_accounts` with the queue of the recorded play mode.
    fn check(&self, program_id: &Pubkey, queue: &Pubkey) -> Result<u8, ProgramError> {
        check_vrf_accounts(
            program_id,
            self.identity,
            self.queue,
            self.vrf,
            self.system,
            self.slot_hashes,
            queue,
        )
    }
}

/// Close the reveal window and freeze the open tick's input once every
/// commitment was revealed or the reveal deadline passed. Offices that
/// committed but did not reveal are logged (`PS_FREEZE`). The tick's
/// randomness is then pending, seeded with the frozen salts' hash: no
/// choice made before this point (salts, which batches to reveal) can know
/// the oracle's answer. The request itself is `RetryTickRandomness` (A19),
/// so this instruction never fails on the oracle's side; the give-up clock
/// starts here. Not subject to the alone rule: it writes no world body and
/// the tick cannot move on until the callback.
pub(super) fn freeze_tick(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let a = VrfAccounts::new(accounts)?;
    signer(a.payer)?;
    let mut meta = world_chunk0(program_id, a.chunk0)?;
    if meta.finished || !meta.revealing || meta.frozen {
        return Err(ChainError::WrongPhase.into());
    }
    let civs = meta.civs as usize;
    let nations = accounts
        .get(7..7 + civs)
        .ok_or(ProgramError::NotEnoughAccountKeys)?;
    let season_info = accounts
        .get(7 + civs)
        .ok_or(ProgramError::NotEnoughAccountKeys)?;
    let mut tick = NO_TICK;
    let (mut committed, mut revealed) = (0usize, 0usize);
    let mut salts: Vec<Salt> = Vec::with_capacity(4 * civs);
    let mut lapsed: Vec<(u16, u8, u32)> = Vec::with_capacity(4 * civs);
    for (civ, info) in nations.iter().enumerate() {
        let n = nation_head(program_id, info, meta.season_id, civ as u16)?;
        if civ > 0 && n.open_tick != tick {
            return Err(ChainError::WrongTick.into());
        }
        tick = n.open_tick;
        for role in Role::ALL {
            let i = role.index();
            if n.committed[i] != n.open_tick {
                continue;
            }
            committed += 1;
            // RevealOrders sets `submitted` with the batch: revealed for
            // this tick exactly when its batch is in the input.
            if n.submitted[i] == n.open_tick {
                revealed += 1;
                salts.push((civ as u16, i as u8, n.salts[i]));
            } else {
                lapsed.push((civ as u16, i as u8, n.officers[i]));
            }
        }
    }
    let now = now()?;
    if now < meta.deadline && revealed < committed {
        return Err(ChainError::TooEarly.into());
    }
    let rand_pre = randomness::salts_hash(meta.season_id, tick, &salts);
    let queue = play_queue(program_id, season_info, meta.season_id)?;
    a.check(program_id, &queue)?;
    for (civ, info) in nations.iter().enumerate() {
        let mut n = nation_head(program_id, info, meta.season_id, civ as u16)?;
        n.frozen = true;
        store_head(info, &n)?;
    }
    meta.frozen = true;
    (meta.input_chunks, meta.input_logged) = (0, 0);
    meta.rand_tick = tick;
    meta.rand_pre = rand_pre;
    meta.rand_out = [0; 32];
    meta.frozen_at = now;
    (meta.rand_requested_at, meta.rand_requests) = (0, 0);
    #[cfg(not(feature = "dev-randomness"))]
    {
        meta.rand_state = RAND_PENDING;
        meta.vrf = [0; 32];
    }
    #[cfg(feature = "dev-randomness")]
    {
        meta.rand_state = randomness::RAND_DEV;
        meta.vrf = randomness::tick_vrf(&rand_pre, &[0; 32], randomness::RAND_DEV)
            .ok_or(ChainError::Rules)?;
    }
    meta.write_chunk0(&mut a.chunk0.try_borrow_mut_data()?)?;
    #[cfg(feature = "dev-randomness")]
    {
        solana_program::log::sol_log_data(&[
            b"PS_RAND",
            &tick.to_le_bytes(),
            &[meta.rand_state],
            &[0; 32],
            &meta.vrf,
        ]);
        msg!("PS randomness dev");
    }
    let record = to_vec_exact(&lapsed)?;
    solana_program::log::sol_log_data(&[b"PS_FREEZE", &tick.to_le_bytes(), &rand_pre, &record]);
    Ok(())
}

/// The VRF's callback with the frozen tick's randomness E: `vrf =
/// tick_vrf(rand_pre, E, RAND_VRF)`. It arrives as a CPI from the VRF
/// program inside the oracle's transaction, so no top-level rule applies
/// (A18): only the VRF program can sign as our scoped identity, and the
/// account must be this season's chunk 0. A stale or duplicate answer (the
/// tick moved on, or an earlier request already answered) changes nothing
/// and succeeds, so the oracle does not retry it.
pub(super) fn consume_tick_randomness(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    e: [u8; 32],
    season_id: u64,
    tick: u16,
) -> ProgramResult {
    let it = &mut accounts.iter();
    let identity = next_account_info(it)?;
    let chunk0 = next_account_info(it)?;
    if !identity.is_signer || *identity.key != randomness::scoped_vrf_identity(program_id) {
        return Err(ChainError::WrongOracle.into());
    }
    let mut meta = world_chunk0(program_id, chunk0)?;
    if meta.season_id != season_id {
        return Err(ChainError::WrongWorld.into());
    }
    if !(meta.frozen && meta.rand_state == RAND_PENDING && meta.rand_tick == tick) {
        msg!("PS stale randomness ignored (tick {})", tick);
        return Ok(());
    }
    let vrf = randomness::tick_vrf(&meta.rand_pre, &e, RAND_VRF).ok_or(ChainError::Rules)?;
    meta.rand_out = e;
    meta.vrf = vrf;
    meta.rand_state = RAND_VRF;
    meta.write_chunk0(&mut chunk0.try_borrow_mut_data()?)?;
    solana_program::log::sol_log_data(&[b"PS_RAND", &tick.to_le_bytes(), &[RAND_VRF], &e, &vrf]);
    Ok(())
}

/// Request the frozen tick's randomness from the VRF (A19: the first
/// request after `FreezeTick`, and again once `VRF_RETRY_SECONDS` passed
/// without an answer; an earlier request stays valid, the first answer
/// wins). Once a request went unanswered until `VRF_GIVEUP_SECONDS` after
/// the freeze: the fallback `tick_vrf(rand_pre, 0, RAND_FALLBACK)`, logged
/// and flagged by the verifier. The queue is checked on every call, the
/// fallback's included, so a wrong queue can never run the clock out.
pub(super) fn retry_tick_randomness(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let a = VrfAccounts::new(accounts)?;
    signer(a.payer)?;
    let mut meta = world_chunk0(program_id, a.chunk0)?;
    if !meta.frozen || meta.rand_state != RAND_PENDING {
        return Err(ChainError::WrongPhase.into());
    }
    let season_info = accounts.get(7).ok_or(ProgramError::NotEnoughAccountKeys)?;
    let queue = play_queue(program_id, season_info, meta.season_id)?;
    let bump = a.check(program_id, &queue)?;
    let now = now()?;
    let asked = meta.rand_requests > 0;
    if asked && now < meta.rand_requested_at.saturating_add(VRF_RETRY_SECONDS) {
        return Err(ChainError::TooEarly.into());
    }
    let tick = meta.rand_tick;
    if asked && now >= meta.frozen_at.saturating_add(VRF_GIVEUP_SECONDS) {
        let vrf = randomness::tick_vrf(&meta.rand_pre, &[0; 32], RAND_FALLBACK)
            .ok_or(ChainError::Rules)?;
        meta.rand_state = RAND_FALLBACK;
        meta.rand_out = [0; 32];
        meta.vrf = vrf;
        meta.write_chunk0(&mut a.chunk0.try_borrow_mut_data()?)?;
        solana_program::log::sol_log_data(&[
            b"PS_RAND",
            &tick.to_le_bytes(),
            &[RAND_FALLBACK],
            &[0; 32],
            &vrf,
        ]);
        msg!("PS tick randomness fallback: the VRF did not answer");
        return Ok(());
    }
    meta.rand_requested_at = now;
    meta.rand_requests = meta.rand_requests.saturating_add(1);
    meta.write_chunk0(&mut a.chunk0.try_borrow_mut_data()?)?;
    msg!(
        "PS tick {} randomness requested ({})",
        tick,
        meta.rand_requests
    );
    request_randomness(
        program_id,
        a.payer,
        a.identity,
        a.queue,
        a.vrf,
        a.system,
        a.slot_hashes,
        bump,
        meta.rand_pre,
        randomness::CONSUME_TICK_TAG,
        a.chunk0.key,
        &randomness::tick_callback_args(meta.season_id, tick),
    )
}

/// Publish the frozen tick's input, `INPUT_CHUNK` bytes per call, as
/// `PS_INPUT` records; chunk 0 also logs the revealed salts (`PS_SALTS`).
pub(super) fn log_tick_input(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    chunk: u16,
) -> ProgramResult {
    let world = world_at(program_id, accounts)?;
    let mut meta = world.meta()?;
    let civs = meta.civs as usize;
    require_alone(program_id, accounts.get(WORLD_CHUNKS + civs))?;
    if meta.finished {
        return Err(ChainError::WrongStatus.into());
    }
    if !meta.frozen {
        return Err(ChainError::WrongPhase.into());
    }
    if meta.rand_state < RAND_VRF {
        return Err(ChainError::RandomnessPending.into());
    }
    let Pending { input, tick, salts } =
        pending_input(program_id, &accounts[WORLD_CHUNKS..], &meta, meta.vrf)?;
    if tick == NO_TICK {
        return Err(ChainError::WrongStatus.into());
    }
    let bytes = to_vec_exact(&input)?;
    drop(input);
    let total = bytes.len().div_ceil(INPUT_CHUNK).max(1) as u16;
    if chunk >= total || chunk > meta.input_logged {
        return Err(ChainError::InvalidParams.into());
    }
    // Every chunk-0 call logs the salts, so the record is in whichever
    // transaction the index keeps; `rand_pre` ties them to the tick's
    // randomness (`randomness::salts_hash`).
    if chunk == 0 {
        let record = to_vec_exact(&salts)?;
        solana_program::log::sol_log_data(&[
            b"PS_SALTS",
            &tick.to_le_bytes(),
            &meta.rand_pre,
            &record,
        ]);
    }
    let hash = hashv(&[&bytes]).to_bytes();
    meta.input_hash = hash;
    meta.input_chunks = total;
    meta.input_logged = meta.input_logged.max(chunk + 1);
    world.set_meta(&meta)?;
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
    #[cfg(feature = "heap-trace")]
    msg!("heap peak {}", crate::heap::peak());
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

/// `ResolveTick { to }`: run the open tick's phases up to `to` (the crank
/// splits a tick into parts that each fit a transaction). With `DEGRADED`
/// (bit 7), once `degrade_after(tick_seconds)` passed the deadline: exactly
/// the next phase, without the tick's orders and governance (a liveness
/// escape for a tick no normal part can finish).
pub(super) fn resolve_tick(program_id: &Pubkey, accounts: &[AccountInfo], to: u8) -> ProgramResult {
    cu!("cu start");
    let world = world_at(program_id, accounts)?;
    let mut meta = world.meta()?;
    let civs = meta.civs as usize;
    require_alone(program_id, accounts.get(WORLD_CHUNKS + civs))?;
    cu!("cu chunks");
    if meta.finished {
        return Err(ChainError::WrongStatus.into());
    }
    // Data availability: a tick resolves only on an input the chain published.
    if !meta.frozen || meta.input_chunks == 0 || meta.input_logged != meta.input_chunks {
        return Err(ChainError::InputNotPublished.into());
    }
    if meta.rand_state < RAND_VRF {
        return Err(ChainError::RandomnessPending.into());
    }
    let degraded = to & DEGRADED != 0;
    if degraded
        && now()?
            < meta
                .deadline
                .saturating_add(degrade_after(meta.tick_seconds))
    {
        return Err(ChainError::TooEarly.into());
    }
    // The world runs the rules this build pins for its preset (the ER has no
    // season account to read the binding from).
    if world.ruleset_hash()? != crate::rules::pinned_ruleset_hash(meta.preset, meta.market)? {
        return Err(ChainError::RulesMismatch.into());
    }
    // The root of the stored body, its copy given back at once; every chunk
    // must come from the same write (the trailer).
    // SAFETY: the result is plain bytes or an error code.
    let pre_root =
        unsafe { scoped(|| world.root().map_err(u64::from)) }.map_err(ProgramError::from)?;
    world.check_root(&pre_root)?;
    cu!("cu hash");
    let mut state = world.read_world()?;
    cu!("cu decode");
    let tick = state.tick;
    let nations = accounts
        .get(WORLD_CHUNKS..WORLD_CHUNKS + civs)
        .ok_or(ProgramError::NotEnoughAccountKeys)?;
    for (civ, info) in nations.iter().enumerate() {
        if nation_head(program_id, info, meta.season_id, civ as u16)?.open_tick != tick {
            return Err(ChainError::WrongTick.into());
        }
    }
    let rules = rules_for(meta.preset, meta.market)?;
    let to_eff = if degraded {
        state.phase_cursor + 1
    } else {
        to.min(PHASE_COUNT)
    };
    // Only phase 0 reads the input's batches and governance.
    let input = if state.phase_cursor == 0 && to_eff > 0 && !degraded {
        pending_input(program_id, nations, &meta, meta.vrf)?.input
    } else {
        TickInput {
            vrf: meta.vrf,
            batches: Vec::new(),
            gov: Vec::new(),
            deposits: Vec::new(),
        }
    };
    cu!("cu input");
    while state.phase_cursor < to_eff {
        let p = state.phase_cursor;
        if degraded {
            run_phase_degraded(&mut state, &rules, meta.vrf, p)
        } else {
            run_phase(&mut state, &rules, &input, p)
        }
        .map_err(|_| ChainError::Rules)?;
        cu!("cu phase");
        if state.phase_cursor == 0 {
            break; // the tick completed
        }
    }
    drop(input);
    // What the record carries of this tick, before a completion resets it.
    let (input_hash, rand_state, rand_pre, rand_out) = (
        meta.input_hash,
        meta.rand_state,
        meta.rand_pre,
        meta.rand_out,
    );
    let completed = state.tick != tick;
    if completed {
        meta.deadline = now()? + meta.tick_seconds as i64;
        meta.finished = state.tick >= rules.ticks_per_season;
        for (civ, info) in nations.iter().enumerate() {
            // SAFETY: the closure returns an error code; the nation account
            // it decodes is stored before the heap is given back.
            let r = unsafe {
                scoped(|| {
                    reopen_nation(program_id, info, &meta, civ as u16, &state, &rules)
                        .map_err(u64::from)
                })
            };
            r.map_err(ProgramError::from)?;
        }
        // Sticky: a completed tick that left USDC unconserved voids the
        // season at FinishSeason, even if a later tick heals the sum.
        if !meta.usdc_broken && !permutation_rules::invariants::usdc_conserved(&state) {
            meta.usdc_broken = true;
            msg!(
                "PS tick {} USDC not conserved: the season will be voided",
                tick
            );
        }
        (meta.frozen, meta.revealing, meta.vrf) = (false, false, [0; 32]);
        (meta.input_chunks, meta.input_logged, meta.input_hash) = (0, 0, [0; 32]);
        (meta.rand_state, meta.rand_tick) = (RAND_NONE, NO_TICK);
        (meta.rand_pre, meta.rand_out) = ([0; 32], [0; 32]);
        (meta.frozen_at, meta.rand_requested_at, meta.rand_requests) = (0, 0, 0);
    }
    cu!("cu nations");
    let root = world.write_world(&state)?;
    world.set_meta(&meta)?;
    cu!("cu write");
    // Tick record for the replay verifier: the step from the previous root
    // to this one, the hash of the input (published in full as PS_INPUT
    // before the tick could resolve) and the tick's randomness. A split tick
    // logs one record per part; a degraded part logs `DEGRADED | stop`.
    let to_byte = if degraded { DEGRADED | to_eff } else { to };
    solana_program::log::sol_log_data(&[
        b"PS_TICK",
        &tick.to_le_bytes(),
        &[to_byte],
        &pre_root,
        &root,
        &input_hash,
        &[rand_state],
        &rand_pre,
        &rand_out,
    ]);
    #[cfg(feature = "heap-trace")]
    msg!("heap peak {}", crate::heap::peak());
    msg!(
        "PS tick {} {}",
        tick,
        if completed { "resolved" } else { "partial" }
    );
    Ok(())
}

/// Opens the next tick in one nation account; after the last tick it stays
/// frozen (nothing is taken while the accounts leave the ER).
fn reopen_nation(
    program_id: &Pubkey,
    info: &AccountInfo,
    meta: &WorldMeta,
    civ: u16,
    state: &WorldState,
    rules: &Ruleset,
) -> ProgramResult {
    let mut n = load_nation(program_id, info, meta.season_id, civ)?;
    open_nation(&mut n, state, rules, meta.deadline);
    n.frozen = meta.finished;
    store(&mut info.try_borrow_mut_data()?, &n)
}
