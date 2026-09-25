//! `resolve_tick` (§15). Phases run in a fixed order; the next phase index is
//! stored in state (`phase_cursor`) so resolution can be split across
//! transactions and resumed by anyone (§15.1, §15.2).
//!
//! Implementation status of each phase is listed on `run_phase`. Each
//! phase lives in its own module; this file only sequences them.

use crate::economy::order_budget;
use crate::gov::{Credit, GovEntry};
use crate::orders::{Order, OrderBatch};
use crate::params::Ruleset;
use crate::rng::Seed;
use crate::state::{CivId, WorldState};
use crate::RulesError;
use alloc::vec::Vec;

mod city_orders;
mod intake;
mod milestones;
mod production;
mod society;

pub use crate::movement::may_enter;
use crate::movement::phase_movement;
pub(crate) use crate::movement::{allied, tile_free_for};
use city_orders::phase_economy_orders;
pub(crate) use city_orders::{item_cost, purchase};
use intake::phase_seed;
use milestones::phase_scoring;
use production::phase_production;
use society::{phase_neutral, phase_society, phase_upkeep};

pub const PHASE_COUNT: u8 = 12;

/// Everything outside the state that a tick consumes. Replaying the same
/// inputs over the same state must give the same root (§17 invariant 7).
#[derive(Clone, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct TickInput {
    /// MagicBlock VRF output for this tick (§0.2).
    pub vrf: Seed,
    /// At most one batch per office per civ is used (the first valid one);
    /// extra or invalid batches are ignored.
    pub batches: Vec<OrderBatch>,
    /// Governance actions, applied in this order in phase 0 (V5 §5.7).
    pub gov: Vec<GovEntry>,
    /// USDC deposited into nation treasuries since the last tick (V5 §7.5).
    pub deposits: Vec<(CivId, u64)>,
}

/// Run all remaining phases of the current tick. Returns the new state root.
pub fn resolve_tick(
    state: &mut WorldState,
    rules: &Ruleset,
    input: &TickInput,
) -> Result<[u8; 32], RulesError> {
    for phase in state.phase_cursor..PHASE_COUNT {
        run_phase(state, rules, input, phase)?;
    }
    state.state_root()
}

/// Run exactly one phase (spec v0.2 §15). Each phase lives in the module named.
///
/// | # | Phase | Module |
/// |---|---|---|
/// | 0 | Seed: tick seed, governance actions, deposits, office batches merged | `tick::intake` |
/// | 1 | Diplomacy: war, peace, NAPs with bonds, alliances, offers | `diplomacy` |
/// | 2 | Economy orders: found, queue, focus, research, purchase, transfers, envoys, gold AMM, USDC market | `tick::city_orders`, `markets` |
/// | 3 | Standing rules: AutoDefend, Retreat, Patrol, AutoPurchase | `standing` |
/// | 4 | Movement: paths, movement points, occupancy, territory, protection | `movement` |
/// | 5 | Combat, captures, razing | `battle` |
/// | 6 | Production and growth | `tick::production` |
/// | 7 | Upkeep | `tick::society` |
/// | 8 | Society: war weariness, loyalty, grievance decay, city regeneration | `tick::society` |
/// | 9 | Neutral actors: city-state growth, suzerainty cycle, the crisis on the leaders (V5 §17.3) | `tick::society` |
/// | 10 | Milestones, eras, office banks, merit | `tick::milestones` |
/// | 11 | Commit: budgets, elections and recalls, event chain | here |
pub fn run_phase(
    state: &mut WorldState,
    rules: &Ruleset,
    input: &TickInput,
    phase: u8,
) -> Result<(), RulesError> {
    if state.tick >= rules.ticks_per_season {
        return Err(RulesError::SeasonOver);
    }
    if phase != state.phase_cursor {
        return Err(RulesError::PhaseOutOfOrder {
            expected: state.phase_cursor,
            got: phase,
        });
    }
    match phase {
        // Only phase 0 reads the input: it merges everything into the state.
        0 => phase_seed(state, rules, input),
        1 => crate::diplomacy::phase_diplomacy(state, rules),
        2 => phase_economy_orders(state, rules),
        3 => crate::standing::phase_standing(state, rules),
        4 => phase_movement(state, rules),
        5 => crate::battle::phase_combat(state, rules),
        6 => phase_production(state, rules),
        7 => phase_upkeep(state),
        8 => phase_society(state, rules),
        9 => phase_neutral(state, rules),
        10 => phase_scoring(state, rules),
        11 => phase_commit(state, rules),
        _ => {
            return Err(RulesError::PhaseOutOfOrder {
                expected: state.phase_cursor,
                got: phase,
            })
        }
    }
    if phase + 1 == PHASE_COUNT {
        state.phase_cursor = 0;
        state.tick += 1;
    } else {
        state.phase_cursor = phase + 1;
    }
    Ok(())
}

/// One accepted order: its civ, the order, its merit credit and its origin
/// (office, position in the office's batch).
pub(crate) type Accepted = (CivId, Order, Credit, (u8, u16));

/// The orders accepted for this tick (merged in phase 0) that `pick`
/// selects, in engine order: civ order, then batch order. They are copied so
/// the phase can change the state as it goes through them; copying only the
/// ones a phase handles keeps phases cheap on chain.
pub(crate) fn accepted(state: &WorldState, pick: impl Fn(&Order) -> bool) -> Vec<Accepted> {
    state
        .tick_orders
        .iter()
        .flat_map(|a| {
            a.iter()
                .filter(|(o, _, _)| pick(o))
                .map(move |(o, credit, origin)| (a.civ, o.clone(), credit, origin))
        })
        .collect()
}

// ---------------------------------------------------------------- phase 11

/// Mark the pairs under a NAP or an alliance this tick: no bounty between
/// them for `bounty_pact_window` ticks (V5 §18.4).
fn record_pacts(state: &mut WorldState) {
    let n = state.civs.len() as CivId;
    for a in 0..n {
        for b in a + 1..n {
            if state.relation(a, b).is_pact() {
                let i = state.pair_index(a, b);
                state.pact_last[i] = Some(state.tick);
            }
        }
    }
}

fn phase_commit(state: &mut WorldState, rules: &Ruleset) {
    for civ in 0..state.civs.len() as u16 {
        let cities = state.city_count(civ);
        let dark = if crate::economy::in_dark_age(state, civ) {
            rules.dark_age_budget
        } else {
            0
        };
        state.civs[civ as usize].tick_budget = order_budget(rules, cities) + dark;
    }
    // Recalls, idle recalls and elections take effect next tick (V5 §5.3–§5.5).
    crate::gov::end_of_tick(state, rules);
    record_pacts(state);
    if state.tick == rules.ai_home_tick {
        // Operator AI home cities are drawn among these (V5 §18.3).
        state.home_snapshot = (0..state.civs.len() as CivId)
            .map(|c| state.living_cities_of(c).map(|x| x.id).collect())
            .collect();
    }
    state.implicit.clear();
    state.tick_orders.clear();
    let tick = state.tick;
    state.push_event(b"tick", &tick.to_le_bytes());
}
