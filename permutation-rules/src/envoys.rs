//! City-state envoys (§12.1): influence sent in phase 2 decides the
//! suzerain of each city-state.

use crate::checks::Blocked;
use crate::fixed::MILLI;
use crate::gov::Credit;
use crate::orders::Order;
use crate::params::Ruleset;
use crate::rng::tie_key;
use crate::state::{CityState, CivId, EnvoyShare, WorldState};
use crate::tick::accepted;

/// Envoy shares (`CityState.envoys`, distinct (civ, credit) pairs) one
/// city-state keeps at most (A15): the list is world state, and credits are
/// as many as the members, so it needs a bound of its own.
pub const MAX_ENVOY_SHARES: usize = 8;

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
            record_share(cs, civ, credit, qty as u64);
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

/// Add `influence` to the (civ, credit) envoy share of `cs`, within
/// `MAX_ENVOY_SHARES` shares. A new pair on a full list merges into the
/// smallest share of the same civ (its merit then goes to that share's
/// credit: the civ's total stays exact). If the civ has none, the civ with
/// the most shares folds its smallest share into its next smallest to make
/// room (that civ's total stays exact too); only when every share belongs to
/// a different civ (more than `MAX_ENVOY_SHARES` nations) is the new share
/// not recorded. Ties go to the earlier share.
fn record_share(cs: &mut CityState, civ: CivId, credit: Credit, influence: u64) {
    if let Some(e) = cs
        .envoys
        .iter_mut()
        .find(|e| e.civ == civ && e.credit == credit)
    {
        e.influence += influence;
        return;
    }
    if cs.envoys.len() >= MAX_ENVOY_SHARES {
        if let Some(i) = smallest_of(&cs.envoys, civ, None) {
            cs.envoys[i].influence += influence;
            return;
        }
        // The civ holding the most shares (the lowest id on a tie).
        let most = cs.envoys.iter().map(|e| e.civ).max_by_key(|c| {
            (
                cs.envoys.iter().filter(|e| e.civ == *c).count(),
                core::cmp::Reverse(*c),
            )
        });
        let Some(most) = most else { return };
        let Some(a) = smallest_of(&cs.envoys, most, None) else {
            return;
        };
        let Some(b) = smallest_of(&cs.envoys, most, Some(a)) else {
            return; // one share per civ: nothing to fold
        };
        cs.envoys[b].influence += cs.envoys[a].influence;
        cs.envoys.remove(a);
    }
    cs.envoys.push(EnvoyShare {
        civ,
        credit,
        influence,
    });
}

/// Index of the smallest envoy share of `civ` (the earlier on a tie),
/// skipping index `except`.
fn smallest_of(envoys: &[EnvoyShare], civ: CivId, except: Option<usize>) -> Option<usize> {
    envoys
        .iter()
        .enumerate()
        .filter(|(i, e)| e.civ == civ && Some(*i) != except)
        .min_by_key(|(i, e)| (e.influence, *i))
        .map(|(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex::Hex;
    use crate::state::Specialty;
    use alloc::vec::Vec;

    fn city_state(shares: &[(CivId, u32, u64)]) -> CityState {
        CityState {
            id: 0,
            hex: Hex::new(0, 0),
            pop: 1,
            defense: 0,
            specialty: Specialty::Mercantile,
            influence: alloc::vec![0; 8],
            suzerain: None,
            captured_by: None,
            envoys: shares
                .iter()
                .map(|(civ, officer, influence)| EnvoyShare {
                    civ: *civ,
                    credit: Credit::officer(*officer),
                    influence: *influence,
                })
                .collect(),
        }
    }

    fn total(cs: &CityState, civ: CivId) -> u64 {
        cs.envoys
            .iter()
            .filter(|e| e.civ == civ)
            .map(|e| e.influence)
            .sum()
    }

    #[test]
    fn envoy_shares_are_capped_and_totals_stay_exact() {
        let full = [
            (0, 0, 30),
            (0, 1, 10),
            (0, 2, 20),
            (1, 3, 5),
            (1, 4, 6),
            (2, 5, 7),
            (2, 6, 8),
            (3, 7, 9),
        ];
        // A known pair adds to its share.
        let mut cs = city_state(&full);
        record_share(&mut cs, 1, Credit::officer(3), 2);
        assert_eq!(cs.envoys[3].influence, 7);
        // A new pair of a civ already there merges into its smallest share.
        record_share(&mut cs, 0, Credit::officer(9), 4);
        assert_eq!(cs.envoys.len(), MAX_ENVOY_SHARES);
        assert_eq!(cs.envoys[1].influence, 14);
        assert_eq!(total(&cs, 0), 64);
        // A civ with no share: the civ with the most shares (0) folds its
        // smallest (14) into its next smallest (20) to make room.
        record_share(&mut cs, 4, Credit::officer(10), 11);
        assert_eq!(cs.envoys.len(), MAX_ENVOY_SHARES);
        assert_eq!(total(&cs, 0), 64);
        assert_eq!(cs.envoys.iter().filter(|e| e.civ == 0).count(), 2);
        assert_eq!(cs.envoys.last().unwrap().credit, Credit::officer(10));
        assert_eq!(total(&cs, 4), 11);
        for civ in 1..4 {
            assert_eq!(
                total(&cs, civ),
                total(&city_state(&full), civ) + u64::from(civ == 1) * 2
            );
        }
        // Eight civs with a share each: a ninth is not recorded.
        let distinct: Vec<(CivId, u32, u64)> = (0..8).map(|c| (c, c as u32, 1)).collect();
        let mut cs = city_state(&distinct);
        record_share(&mut cs, 8, Credit::officer(99), 5);
        assert_eq!(cs.envoys.len(), MAX_ENVOY_SHARES);
        assert_eq!(total(&cs, 8), 0);
    }

    #[test]
    fn many_envoys_keep_the_list_bounded() {
        let rules = Ruleset::new(crate::params::Preset::Blitz);
        let mut s = crate::genesis::new_season(
            &rules,
            &[1; 32],
            &[2; 32],
            &crate::genesis::nation_entries(6),
        )
        .unwrap();
        for c in &mut s.civs {
            c.influence = 1_000 * MILLI;
        }
        // Six nations, every one sending through twenty different officers.
        s.tick_orders = (0..6u16)
            .map(|civ| crate::orders::CivOrders {
                civ,
                orders: (0..20)
                    .map(|_| Order::SendEnvoy {
                        city_state: 0,
                        influence: 1,
                    })
                    .collect(),
                credits: (0..20)
                    .map(|k| Credit::officer(100 * civ as u32 + k))
                    .collect(),
                origin: (0..20).map(|k| (3, k)).collect(),
                spent: [0; 4],
            })
            .collect();
        apply_envoys(&mut s, &rules);
        let cs = &s.city_states[0];
        assert!(cs.envoys.len() <= MAX_ENVOY_SHARES);
        let sent: u64 = cs.envoys.iter().map(|e| e.influence).sum();
        assert_eq!(sent, 120 * MILLI as u64, "every envoy is counted");
        for civ in 0..6 {
            assert_eq!(total(cs, civ), 20 * MILLI as u64);
        }
    }
}
