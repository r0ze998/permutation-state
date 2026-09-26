//! Invariants that MUST hold after every tick (spec v0.2 §17), checked by the
//! tests, the season simulation and the replay verifier. The freezes
//! (invariant 8) and the per-office budget (invariant 4) are enforced where
//! orders are accepted (`orders::validate_batch`), so they are not rechecked
//! here.

use crate::params::Ruleset;
use crate::state::WorldState;
use alloc::vec::Vec;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub invariant: u8,
    pub what: &'static str,
    pub id: u64,
}

/// USDC the world holds (treasuries, contract escrow, exchange vault and
/// operations), summed without overflow (WP12: the chain checks it too).
pub fn usdc_held(state: &WorldState) -> u128 {
    state.civs.iter().map(|c| c.usdc as u128).sum::<u128>()
        + state
            .contracts
            .iter()
            .map(|c| c.escrow as u128)
            .sum::<u128>()
        + state.exchange_vault as u128
        + state.exchange_ops as u128
}

/// Invariant 1: every USDC base unit deposited is held somewhere, exactly.
pub fn usdc_conserved(state: &WorldState) -> bool {
    usdc_held(state) == state.usdc_deposited as u128
}

/// Check every invariant that depends only on the current state.
pub fn check(state: &WorldState, rules: &Ruleset) -> Vec<Violation> {
    let mut v = Vec::new();

    // 2. Occupancy: ≤ 1 army and ≤ 1 civilian per tile; both only in an own city.
    let alive: Vec<_> = state.units.iter().filter(|u| u.alive).collect();
    for (i, a) in alive.iter().enumerate() {
        for b in &alive[i + 1..] {
            if a.hex != b.hex {
                continue;
            }
            let own_city = state.cities.iter().any(|c| {
                c.alive
                    && c.hex == a.hex
                    && matches!(a.owner, crate::state::Owner::Civ(x) if c.owner == Some(x))
            });
            let mixed = a.unit_type.is_civilian() != b.unit_type.is_civilian();
            if !(a.owner == b.owner && own_city && mixed) {
                v.push(Violation {
                    invariant: 2,
                    what: "tile shared illegally",
                    id: a.id as u64,
                });
            }
        }
    }

    // 3. Army size bounds.
    for u in &alive {
        if !u.unit_type.is_civilian()
            && !(rules.army_min_milli..=rules.army_cap_milli).contains(&u.troops)
        {
            v.push(Violation {
                invariant: 3,
                what: "army size out of bounds",
                id: u.id as u64,
            });
        }
    }

    // 5. No negative stocks.
    for c in &state.civs {
        if c.gold < 0 || c.science_store < 0 || c.influence < 0 || c.iron < 0 || c.horses < 0 {
            v.push(Violation {
                invariant: 5,
                what: "negative civ stock",
                id: c.id as u64,
            });
        }
    }
    for c in &state.cities {
        if c.food < 0 || c.prod < 0 {
            v.push(Violation {
                invariant: 5,
                what: "negative city store",
                id: c.id as u64,
            });
        }
    }

    // 10. Technologies never held without prerequisites.
    for c in &state.civs {
        for t in crate::tech::TECHS {
            if c.techs.has(t.tech) && !c.techs.prereqs_met(t.tech) {
                v.push(Violation {
                    invariant: 10,
                    what: "tech without prerequisites",
                    id: c.id as u64,
                });
            }
        }
    }

    // 1. USDC conservation on the ER: balances + Vault + operations = deposits.
    // In u128: a wrapped balance must not hide in a wrapped sum (audit WP08).
    if !usdc_conserved(state) {
        v.push(Violation {
            invariant: 1,
            what: "USDC not conserved",
            id: u64::try_from(usdc_held(state)).unwrap_or(u64::MAX),
        });
    }
    // V5: at most one ballot per member (WP06).
    if state.ballots.len() > state.members.len() {
        v.push(Violation {
            invariant: 12,
            what: "ballots out of step with members",
            id: state.ballots.len() as u64,
        });
    }
    // V5: offices are held by members of the nation, at most
    // `max_offices_per_member` each (V5 §5.1, D12).
    for (civ, n) in state.nations.iter().enumerate() {
        for m in n.offices {
            if m == crate::gov::NOBODY {
                continue;
            }
            if !crate::gov::is_member_of(state, m, civ as u16) {
                v.push(Violation {
                    invariant: 12,
                    what: "office held by a non-member",
                    id: m as u64,
                });
            }
            if n.offices_held(m) > rules.max_offices_per_member as usize {
                v.push(Violation {
                    invariant: 12,
                    what: "too many offices",
                    id: m as u64,
                });
            }
        }
    }
    // 11. Territory stays within the largest territory radius of its city (§5.4).
    for t in &state.map.tiles {
        if let Some(c) = t.owner_city.and_then(|c| state.cities.get(c as usize)) {
            if c.hex.distance(t.hex) > crate::map::MAX_TERRITORY_RADIUS {
                v.push(Violation {
                    invariant: 11,
                    what: "tile owned beyond territory radius",
                    id: c.id as u64,
                });
            }
        }
    }
    v
}

/// Invariants 6 and 10 across a tick: achievement records, merit and tech
/// counts never decrease.
pub fn check_monotonic(before: &WorldState, after: &WorldState) -> Vec<Violation> {
    let mut v = Vec::new();
    for (a, b) in before.members.iter().zip(after.members.iter()) {
        if a.merit.iter().zip(b.merit.iter()).any(|(x, y)| y < x)
            || b.windows & a.windows != a.windows
        {
            v.push(Violation {
                invariant: 6,
                what: "merit or activity decreased",
                id: 0,
            });
        }
    }
    for (a, b) in before.civs.iter().zip(after.civs.iter()) {
        let (s0, s1) = (&a.achievements, &b.achievements);
        if s1.wealth < s0.wealth
            || s1.star_gate_max < s0.star_gate_max
            || (s0.ever_suzerain && !s1.ever_suzerain)
            || (s0.envoy_sent && !s1.envoy_sent)
            || s1.trade.iter().zip(&s0.trade).any(|(y, x)| y < x)
            || b.scores.science_total < a.scores.science_total
        {
            v.push(Violation {
                invariant: 6,
                what: "score decreased",
                id: a.id as u64,
            });
        }
        if b.techs.count() < a.techs.count() {
            v.push(Violation {
                invariant: 10,
                what: "tech count decreased",
                id: a.id as u64,
            });
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis::{nation_entries, new_season};
    use crate::params::{Preset, Ruleset};

    #[test]
    fn a_wrapped_treasury_is_not_conserved() {
        let rules = Ruleset::new(Preset::Blitz);
        let mut entries = nation_entries(2);
        for e in &mut entries {
            e.treasury = 10;
        }
        let mut s = new_season(&rules, &[1; 32], &[2; 32], &entries).unwrap();
        assert!(usdc_conserved(&s));
        assert!(check(&s, &rules).iter().all(|v| v.invariant != 1));
        // One treasury wrapped below zero (u64::MAX) and the other +11: the
        // u64 sum still equals the deposits, the u128 sum does not.
        s.civs[0].usdc = u64::MAX;
        s.civs[1].usdc += 11;
        let wrapped = s.civs.iter().fold(0u64, |a, c| a.wrapping_add(c.usdc));
        assert_eq!(wrapped, s.usdc_deposited);
        assert!(!usdc_conserved(&s));
        assert!(check(&s, &rules).iter().any(|v| v.invariant == 1));
    }
}
