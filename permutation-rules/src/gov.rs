//! Nations and their governance (Game Design V5 §4–§5).
//!
//! Every season has a fixed set of nations (the civs of the world). Players,
//! human or AI, join one nation before the season as **members**. Members
//! elect four **officers** (general, steward, science, diplomat) every
//! `term_ticks`; only an office holder's orders reach the world, each office
//! within its own domain and budget. Other members take part through
//! **proposals** (an officer may adopt them, sharing the merit), **support**
//! for proposals, **votes** and **recalls**.
//!
//! Governance is part of the deterministic world: every action arrives in a
//! tick's input (`TickInput::gov`), is applied in phase 0, and elections and
//! recalls resolve in phase 11 for the next tick. Replaying the inputs
//! therefore replays who held which office and why, exactly like the orders.
//!
//! An office without a holder is run by the **acting official** (`NOBODY`,
//! the bot "代行"): its orders count, but earn no merit.

use crate::orders::Order;
use crate::params::Ruleset;
use crate::state::{CivId, WorldState};
use crate::RulesError;
use alloc::vec::Vec;
use borsh::{BorshDeserialize, BorshSerialize};

pub type MemberId = u32;
/// No member: a vacant office (run by the acting official) or no proposer.
pub const NOBODY: MemberId = u32::MAX;
/// A tick that never happened.
pub const NEVER: u16 = u16::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub enum Role {
    /// Armies and scouts: move, attack, raze, unit standing rules.
    General,
    /// Cities and settlers: queue, focus, purchase, found city, city standing rules.
    Steward,
    /// Research.
    Science,
    /// Other nations, city-states and trade.
    Diplomat,
}

impl Role {
    pub const ALL: [Role; 4] = [Role::General, Role::Steward, Role::Science, Role::Diplomat];

    pub const fn index(self) -> usize {
        self as usize
    }

    pub const fn bit(self) -> u8 {
        1 << self as u8
    }

    pub fn from_index(i: usize) -> Option<Role> {
        Role::ALL.get(i).copied()
    }
}

/// Who gets the merit of an order: the officer who issued it and, if it
/// came from an adopted proposal, the proposer (half each, V5 §5.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Credit {
    pub officer: MemberId,
    pub proposer: MemberId,
}

impl Credit {
    pub const NONE: Credit = Credit { officer: NOBODY, proposer: NOBODY };

    pub const fn officer(m: MemberId) -> Credit {
        Credit { officer: m, proposer: NOBODY }
    }

    pub fn is_none(&self) -> bool {
        self.officer == NOBODY && self.proposer == NOBODY
    }
}

impl Default for Credit {
    fn default() -> Self {
        Credit::NONE
    }
}

/// Merit paths (V5 §7.3). `Common` holds merit that belongs to no path
/// (office duty); it counts toward the era bonus share only.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Path {
    Hegemony,
    Prosperity,
    Science,
    Concord,
    Common,
}

pub const PATHS: usize = 4;

/// One member of one nation.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Member {
    pub civ: CivId,
    /// The key that signs this member's governance actions and, in office,
    /// its orders (a session key; the wallet stays on the base layer).
    pub key: [u8; 32],
    /// Bit `w` set = active in activity window `w` (V5 §7.3).
    pub windows: u32,
    pub last_active: u16,
    /// Offices this member stands for (`Role::bit`).
    pub standing_for: u8,
    /// Merit in milli-merit per `Path`.
    pub merit: [u32; 5],
}

impl Member {
    pub fn merit_total(&self) -> u64 {
        self.merit.iter().map(|m| *m as u64).sum()
    }

    pub fn active_windows(&self) -> u32 {
        self.windows.count_ones()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Vote {
    pub voter: MemberId,
    pub role: Role,
    pub candidate: MemberId,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Recall {
    pub role: Role,
    /// The officer being recalled; the recall lapses if the office changes hands.
    pub holder: MemberId,
    pub opened: u16,
    /// Opened automatically because the officer was idle (V5 §5.5).
    pub automatic: bool,
    pub yes: Vec<MemberId>,
}

/// A member's proposed orders for one office (献策, V5 §5.4).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct GovProposal {
    pub id: u32,
    pub role: Role,
    pub proposer: MemberId,
    pub tick: u16,
    pub orders: Vec<Order>,
    pub supporters: Vec<MemberId>,
    /// Adopted this tick; removed at commit.
    pub adopted: bool,
}

/// A nation's government (one per civ).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Nation {
    /// Holder of each office (`Role::index`), `NOBODY` if vacant.
    pub offices: [MemberId; 4],
    /// First tick of the holder's continuous tenure.
    pub office_since: [u16; 4],
    /// Last tick the holder's batch carried an order other than a reveal
    /// (merit credited to "the officer" needs a recent one).
    pub office_last_act: [u16; 4],
    /// Last tick the holder sealed a batch at all, even an empty one: the
    /// idle clock of the automatic recall, which is for abandoned offices,
    /// not for officers with nothing to order (V5 §5.5).
    pub office_seen: [u16; 4],
    /// Runner-up of the last election, the successor if the holder is recalled.
    pub runner_up: [MemberId; 4],
    /// Unused budget carried per office (§4.1 bank, per role).
    pub role_bank: [u16; 4],
    pub members: u32,
    /// Votes for the coming term.
    pub votes: Vec<Vote>,
    pub recalls: Vec<Recall>,
    pub proposals: Vec<GovProposal>,
    pub next_proposal: u32,
    /// Proposals adopted so far this season.
    pub adopted: u32,
}

impl Nation {
    pub fn new() -> Self {
        Nation {
            offices: [NOBODY; 4],
            office_since: [0; 4],
            office_last_act: [NEVER; 4],
            office_seen: [NEVER; 4],
            runner_up: [NOBODY; 4],
            role_bank: [0; 4],
            members: 0,
            votes: Vec::new(),
            recalls: Vec::new(),
            proposals: Vec::new(),
            next_proposal: 0,
            adopted: 0,
        }
    }

    pub fn holder(&self, role: Role) -> MemberId {
        self.offices[role.index()]
    }

    pub fn offices_held(&self, m: MemberId) -> usize {
        self.offices.iter().filter(|o| **o == m).count()
    }

    pub fn proposal(&self, id: u32) -> Option<&GovProposal> {
        self.proposals.iter().find(|p| p.id == id)
    }
}

impl Default for Nation {
    fn default() -> Self {
        Nation::new()
    }
}

/// A governance action by a member (V5 §5.7). All of them are recorded on
/// chain and replayed from the tick inputs.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum GovAction {
    /// Stand for these offices (`Role::bit` mask); 0 withdraws.
    Stand { roles: u8 },
    /// One vote per office for the coming term, during the vote window.
    Vote { role: Role, candidate: MemberId },
    /// Propose orders for an office.
    Propose { role: Role, orders: Vec<Order> },
    Support { proposal: u32 },
    /// Open a recall of the office's holder, or vote for the open one.
    Recall { role: Role },
}

impl GovAction {
    /// Votes, proposals, support and recall votes make a member active
    /// (V5 §7.3); standing for office does not.
    pub fn counts_as_activity(&self) -> bool {
        !matches!(self, GovAction::Stand { .. })
    }
}

/// One governance action as it arrives in a tick input: the chain program
/// records who signed it, and the engine checks that against the member.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct GovEntry {
    pub member: MemberId,
    pub signer: [u8; 32],
    pub action: GovAction,
}

// ------------------------------------------------------------------ membership

/// Register a member before the season starts (V5 §4: no late entry). The
/// id is the registration index; the chain program assigns the same one.
pub fn join(state: &mut WorldState, rules: &Ruleset, civ: CivId, key: [u8; 32]) -> Result<MemberId, RulesError> {
    if state.tick != 0 || state.phase_cursor != 0 {
        return Err(RulesError::RegistrationClosed);
    }
    if civ as usize >= state.civs.len() {
        return Err(RulesError::UnknownCiv(civ));
    }
    if state.members.len() >= rules.max_members as usize {
        return Err(RulesError::NationFull);
    }
    if state.members.iter().any(|m| m.key == key) {
        return Err(RulesError::AlreadyMember);
    }
    let id = state.members.len() as MemberId;
    state.members.push(Member {
        civ,
        key,
        windows: 0,
        last_active: NEVER,
        standing_for: 0,
        merit: [0; 5],
    });
    state.nations[civ as usize].members += 1;
    Ok(id)
}

/// Apply governance actions taken during registration: standing, and the
/// votes of the first election. Votes may name members who join later, so
/// the candidate is checked at the election, not here.
pub fn apply_pre_season(state: &mut WorldState, rules: &Ruleset, entries: &[GovEntry]) -> Result<(), RulesError> {
    if state.tick != 0 || state.phase_cursor != 0 {
        return Err(RulesError::RegistrationClosed);
    }
    for e in entries {
        apply(state, rules, e, true);
    }
    Ok(())
}

/// Hold the first election, for the term starting at tick 0. Call once,
/// after every `join`, before tick 0 resolves.
pub fn first_election(state: &mut WorldState, rules: &Ruleset) -> Result<(), RulesError> {
    if state.tick != 0 || state.phase_cursor != 0 {
        return Err(RulesError::RegistrationClosed);
    }
    run_election(state, rules, 0);
    Ok(())
}

/// `apply_pre_season` then `first_election`.
pub fn open_government(state: &mut WorldState, rules: &Ruleset, entries: &[GovEntry]) -> Result<(), RulesError> {
    apply_pre_season(state, rules, entries)?;
    first_election(state, rules)
}

pub fn is_member_of(state: &WorldState, m: MemberId, civ: CivId) -> bool {
    state.members.get(m as usize).is_some_and(|x| x.civ == civ)
}

/// Mark `m` active this tick (activity window and recall electorate).
pub fn mark_active(state: &mut WorldState, rules: &Ruleset, m: MemberId) {
    let tick = state.tick;
    if let Some(x) = state.members.get_mut(m as usize) {
        let w = tick / rules.activity_window_ticks.max(1);
        if w < 32 {
            x.windows |= 1 << w;
        }
        x.last_active = tick;
    }
}

/// Active for the payout (V5 §7.3): active in at least `active_windows_needed` windows.
pub fn is_active_member(rules: &Ruleset, m: &Member) -> bool {
    m.active_windows() >= rules.active_windows_needed as u32
}

/// The holder of `role` if it is a member who executed an order recently,
/// for merit credited to "the officer" (V5 §7.3).
pub fn active_officer(state: &WorldState, rules: &Ruleset, civ: CivId, role: Role) -> Option<MemberId> {
    let n = state.nations.get(civ as usize)?;
    let m = n.offices[role.index()];
    let last = n.office_last_act[role.index()];
    (m != NOBODY && last != NEVER && state.tick.saturating_sub(last) < rules.officer_active_ticks).then_some(m)
}

// ------------------------------------------------------------------ phase 0: actions

/// First tick of the term after `tick`, if the season has one.
pub fn next_term_start(rules: &Ruleset, tick: u16) -> Option<u16> {
    let term = rules.term_ticks.max(1);
    let next = (tick / term + 1) * term;
    (next < rules.ticks_per_season).then_some(next)
}

/// Whether votes for the coming term are accepted at `tick`.
pub fn vote_open(rules: &Ruleset, tick: u16) -> bool {
    next_term_start(rules, tick).is_some_and(|t| t - tick <= rules.vote_window)
}

/// Phase 0: apply this tick's governance actions in input order.
pub fn apply_actions(state: &mut WorldState, rules: &Ruleset, entries: &[GovEntry]) {
    for e in entries {
        apply(state, rules, e, false);
    }
}

fn apply(state: &mut WorldState, rules: &Ruleset, e: &GovEntry, pre_season: bool) {
    let Some(member) = state.members.get(e.member as usize) else {
        return;
    };
    if member.key != e.signer {
        return;
    }
    let civ = member.civ;
    let ok = match &e.action {
        GovAction::Stand { roles } => {
            state.members[e.member as usize].standing_for = roles & 0x0f;
            true
        }
        GovAction::Vote { role, candidate } => {
            let ok = if pre_season { true } else { vote_open(rules, state.tick) && is_member_of(state, *candidate, civ) };
            if ok {
                let n = &mut state.nations[civ as usize];
                n.votes.retain(|v| !(v.voter == e.member && v.role == *role));
                n.votes.push(Vote { voter: e.member, role: *role, candidate: *candidate });
                true
            } else {
                false
            }
        }
        GovAction::Propose { role, orders } => propose(state, rules, civ, e.member, *role, orders),
        GovAction::Support { proposal } => {
            let n = &mut state.nations[civ as usize];
            match n.proposals.iter_mut().find(|p| p.id == *proposal) {
                Some(p) if p.proposer != e.member && !p.supporters.contains(&e.member) => {
                    p.supporters.push(e.member);
                    true
                }
                _ => false,
            }
        }
        GovAction::Recall { role } => recall_vote(state, civ, e.member, *role),
    };
    if ok && e.action.counts_as_activity() {
        mark_active(state, rules, e.member);
    }
}

fn propose(state: &mut WorldState, rules: &Ruleset, civ: CivId, m: MemberId, role: Role, orders: &[Order]) -> bool {
    if orders.is_empty() || orders.len() > rules.max_proposal_orders as usize {
        return false;
    }
    if crate::orders::check_structure(rules, state.tick, orders).is_err() {
        return false;
    }
    if !orders.iter().all(|o| crate::orders::role_allows(state, role, o)) {
        return false;
    }
    let n = &mut state.nations[civ as usize];
    if n.proposals.len() >= rules.max_open_proposals as usize {
        return false;
    }
    let id = n.next_proposal;
    n.next_proposal += 1;
    n.proposals.push(GovProposal {
        id,
        role,
        proposer: m,
        tick: state.tick,
        orders: orders.to_vec(),
        supporters: Vec::new(),
        adopted: false,
    });
    true
}

fn recall_vote(state: &mut WorldState, civ: CivId, m: MemberId, role: Role) -> bool {
    let tick = state.tick;
    let n = &mut state.nations[civ as usize];
    let holder = n.offices[role.index()];
    if holder == NOBODY {
        return false;
    }
    match n.recalls.iter_mut().find(|r| r.role == role && r.holder == holder) {
        Some(r) if r.yes.contains(&m) => false,
        Some(r) => {
            r.yes.push(m);
            true
        }
        None => {
            n.recalls.push(Recall { role, holder, opened: tick, automatic: false, yes: alloc::vec![m] });
            true
        }
    }
}

// ------------------------------------------------------------------ phase 11: terms

/// Phase 11 of tick `t`: resolve recalls, open automatic recalls, hold the
/// election for a term starting at `t + 1`, and expire proposals. Changes
/// take effect from tick `t + 1`.
pub fn end_of_tick(state: &mut WorldState, rules: &Ruleset) {
    let t = state.tick;
    for civ in 0..state.nations.len() {
        resolve_recalls(state, rules, civ as CivId);
        open_idle_recalls(state, rules, civ as CivId);
    }
    if (t + 1) % rules.term_ticks.max(1) == 0 && t + 1 < rules.ticks_per_season {
        run_election(state, rules, t + 1);
    }
    for n in &mut state.nations {
        n.proposals.retain(|p| !p.adopted && t + 1 - p.tick < rules.proposal_ttl_ticks);
    }
}

fn electorate(state: &WorldState, rules: &Ruleset, civ: CivId) -> usize {
    let t = state.tick;
    state
        .members
        .iter()
        .filter(|m| m.civ == civ && m.last_active != NEVER && t - m.last_active < rules.recall_electorate_ticks)
        .count()
}

fn resolve_recalls(state: &mut WorldState, rules: &Ruleset, civ: CivId) {
    let t = state.tick;
    let voters = electorate(state, rules, civ);
    let recalls = core::mem::take(&mut state.nations[civ as usize].recalls);
    let mut keep = Vec::new();
    for r in recalls {
        let n = &state.nations[civ as usize];
        if n.offices[r.role.index()] != r.holder {
            continue; // the office changed hands: the recall lapses
        }
        if r.yes.len() * 2 > voters {
            vacate(state, rules, civ, r.role);
            let mut payload = [0u8; 7];
            payload[..2].copy_from_slice(&civ.to_le_bytes());
            payload[2] = r.role as u8;
            payload[3..].copy_from_slice(&r.holder.to_le_bytes());
            state.push_event(b"recalled", &payload);
            continue;
        }
        if t + 1 - r.opened < rules.recall_ticks {
            keep.push(r);
        }
    }
    state.nations[civ as usize].recalls = keep;
}

/// Remove the holder; the last election's runner-up succeeds, else the acting official.
fn vacate(state: &mut WorldState, rules: &Ruleset, civ: CivId, role: Role) {
    let next = state.tick + 1;
    let n = &mut state.nations[civ as usize];
    let i = role.index();
    let old = n.offices[i];
    let r = n.runner_up[i];
    n.offices[i] = NOBODY;
    let successor = if r != NOBODY && r != old && n.offices_held(r) < rules.max_offices_per_member as usize {
        r
    } else {
        NOBODY
    };
    n.offices[i] = successor;
    n.runner_up[i] = NOBODY;
    n.office_since[i] = next;
    n.office_last_act[i] = NEVER;
    n.office_seen[i] = NEVER;
}

fn open_idle_recalls(state: &mut WorldState, rules: &Ruleset, civ: CivId) {
    let t = state.tick;
    let n = &mut state.nations[civ as usize];
    for role in Role::ALL {
        let i = role.index();
        let holder = n.offices[i];
        if holder == NOBODY || n.recalls.iter().any(|r| r.role == role) {
            continue;
        }
        let since = match n.office_seen[i] {
            NEVER => n.office_since[i],
            last => last.max(n.office_since[i]),
        };
        if t + 1 >= since && t + 1 - since >= rules.idle_recall_ticks {
            n.recalls.push(Recall { role, holder, opened: t, automatic: true, yes: Vec::new() });
        }
    }
}

/// Elect every office of every nation for the term starting at `start`
/// (V5 §5.3). Offices are filled in `Role::ALL` order; a member already
/// holding `max_offices_per_member` offices is passed over. Most votes wins;
/// ties go to the on-chain random tie-break. A candidate with no votes can
/// still win an uncontested office.
pub fn run_election(state: &mut WorldState, rules: &Ruleset, start: u16) {
    let seed = crate::hash::sha256(&[b"PS/election", &state.season_seed, &state.tick_seed, &start.to_le_bytes()]);
    for civ in 0..state.nations.len() {
        let civ_id = civ as CivId;
        let mut new_offices = [NOBODY; 4];
        let mut runners = [NOBODY; 4];
        for role in Role::ALL {
            let mut ranked: Vec<(u32, u64, MemberId)> = state
                .members
                .iter()
                .enumerate()
                .filter(|(_, m)| m.civ == civ_id && m.standing_for & role.bit() != 0)
                .map(|(id, _)| {
                    let id = id as MemberId;
                    let votes = state.nations[civ].votes.iter().filter(|v| v.role == role && v.candidate == id).count() as u32;
                    let key = ((civ as u64) << 40) | ((role as u64) << 32) | id as u64;
                    (votes, crate::rng::tie_key(&seed, key), id)
                })
                .collect();
            ranked.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
            let eligible: Vec<MemberId> = ranked
                .into_iter()
                .map(|(_, _, id)| id)
                .filter(|id| new_offices.iter().filter(|o| *o == id).count() < rules.max_offices_per_member as usize)
                .collect();
            new_offices[role.index()] = eligible.first().copied().unwrap_or(NOBODY);
            runners[role.index()] = eligible.get(1).copied().unwrap_or(NOBODY);
        }
        let n = &mut state.nations[civ];
        // A re-elected holder keeps its tenure (and its idle clock, and any
        // open recall); a new holder starts fresh. Recalls of replaced
        // holders lapse on their own.
        for (i, new) in new_offices.iter().enumerate() {
            if n.offices[i] != *new {
                n.office_last_act[i] = NEVER;
                n.office_seen[i] = NEVER;
                n.office_since[i] = start;
            }
        }
        n.offices = new_offices;
        n.runner_up = runners;
        n.votes.clear();
        for (i, m) in new_offices.iter().enumerate() {
            let mut payload = [0u8; 9];
            payload[..2].copy_from_slice(&civ_id.to_le_bytes());
            payload[2] = i as u8;
            payload[3..7].copy_from_slice(&m.to_le_bytes());
            payload[7..].copy_from_slice(&start.to_le_bytes());
            state.push_event(b"elected", &payload);
        }
    }
}
