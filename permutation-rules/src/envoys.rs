//! City-state envoys (§12.1): influence sent in phase 2 decides the
//! suzerain of each city-state.

use crate::checks::Blocked;
use crate::fixed::MILLI;
use crate::orders::Order;
use crate::params::Ruleset;
use crate::rng::tie_key;
use crate::state::{CivId, EnvoyShare, WorldState};
use crate::tick::accepted;

/// Envoys (§12.1): all envoys of the tick are applied, then each unclaimed
/// city-state goes to the civ with the most influence at or above the
/// threshold (ties by tie-break), so civ order never decides suzerainty.
pub fn apply_envoys(state: &mut WorldState, rules: &Ruleset) {
    for (civ, order, credit, origin) in accepted(state, |o| matches!(o, Order::SendEnvoy { .. })) {
        if let Order::SendEnvoy {
            city_state,
            influence,
        } = order
        {
            let qty = influence as i64 * MILLI;
            let open = state
                .city_states
                .get(city_state as usize)
                .is_some_and(|cs| cs.captured_by.is_none());
            let result = if !open {
                Err(Blocked::TargetGone)
            } else if qty <= 0 || state.civs[civ as usize].influence < qty {
                Err(Blocked::NotEnoughInfluence)
            } else {
                Ok(())
            };
            if let Err(why) = result {
                state.skip(civ, origin, why.code());
                continue;
            }
            state.civs[civ as usize].influence -= qty;
            state.civs[civ as usize].achievements.envoy_sent = true;
            let cs = &mut state.city_states[city_state as usize];
            cs.influence[civ as usize] += qty;
            match cs
                .envoys
                .iter_mut()
                .find(|e| e.civ == civ && e.credit == credit)
            {
                Some(e) => e.influence += qty as u64,
                None => cs.envoys.push(EnvoyShare {
                    civ,
                    credit,
                    influence: qty as u64,
                }),
            }
        }
    }
    let threshold = rules.suzerain_threshold as i64 * MILLI;
    let seed = state.tick_seed;
    for cs in &mut state.city_states {
        if cs.suzerain.is_some() || cs.captured_by.is_some() {
            continue;
        }
        cs.suzerain = cs
            .influence
            .iter()
            .enumerate()
            .filter(|(_, v)| **v >= threshold)
            .max_by_key(|(c, v)| (**v, core::cmp::Reverse(tie_key(&seed, *c as u64))))
            .map(|(c, _)| c as CivId);
    }
}
