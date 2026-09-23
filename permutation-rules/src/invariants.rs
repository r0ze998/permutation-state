//! Invariants that MUST hold after every tick (§17). Used by tests and the
//! replay verifier. Invariant 1 (USDC conservation) and 8 (freezes) are
//! enforced where USDC and transfers are implemented; see TODOs.

use crate::params::Ruleset;
use crate::state::WorldState;
use alloc::vec::Vec;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Violation {
    pub invariant: u8,
    pub what: &'static str,
    pub id: u64,
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
    let held: u64 =
        state.civs.iter().map(|c| c.usdc).sum::<u64>() + state.exchange_vault + state.exchange_ops;
    if held != state.usdc_deposited {
        v.push(Violation {
            invariant: 1,
            what: "USDC not conserved",
            id: held,
        });
    }
    // Exchange season spend cap (§11.3).
    let cap = rules.entry_fee_usdc * rules.exchange_spend_cap_bps as u64 / 10_000;
    for c in &state.civs {
        if c.exchange_spent > cap {
            v.push(Violation {
                invariant: 1,
                what: "Exchange spend cap exceeded",
                id: c.id as u64,
            });
        }
    }
    // TODO(§17 #4): budget per applied orders is enforced by `validate_batch`.
    v
}

/// Invariants 6 and 10 across a tick: scores and tech counts never decrease.
pub fn check_monotonic(before: &WorldState, after: &WorldState) -> Vec<Violation> {
    let mut v = Vec::new();
    for (a, b) in before.civs.iter().zip(after.civs.iter()) {
        let (s0, s1) = (&a.scores, &b.scores);
        if s1.dominion < s0.dominion
            || s1.concord_raw < s0.concord_raw
            || s1.science_total < s0.science_total
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
