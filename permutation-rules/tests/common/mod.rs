//! Helpers shared by the integration tests (`mod common;`).
#![allow(dead_code)] // each test crate uses a subset

use permutation_rules::gov::{Member, MemberId, Role, NEVER, NOBODY};
use permutation_rules::hex::Hex;
use permutation_rules::invariants;
use permutation_rules::orders::{office_batches, Order};
use permutation_rules::rng::Seed;
use permutation_rules::state::WorldState;
use permutation_rules::tick::{resolve_tick, TickInput};
use permutation_rules::Ruleset;

pub mod nations;
pub fn vrf(tick: u16) -> Seed {
    let mut s = [0u8; 32];
    s[..2].copy_from_slice(&tick.to_le_bytes());
    s
}
/// Seat test officers in every nation whose offices are vacant: one member
/// holds general and steward, another science and diplomat (at most two
/// offices each, V5 D12; war consent needs a second person, V5 §5.6). A
/// vacant office takes no batch (the caretaker fills it), so tests that
/// order a nation around need officers.
pub fn staff(s: &mut WorldState) {
    for civ in 0..s.civs.len() {
        for pair in [
            [Role::General, Role::Steward],
            [Role::Science, Role::Diplomat],
        ] {
            let n = &s.nations[civ];
            if pair.iter().any(|r| n.holder(*r) != NOBODY) {
                continue;
            }
            let id = s.members.len() as MemberId;
            let mut key = [0xAB; 32];
            key[..4].copy_from_slice(&id.to_le_bytes());
            s.members.push(Member {
                civ: civ as u16,
                key,
                windows: 0,
                last_active: NEVER,
                standing_for: 0,
                merit: [0; 5],
            });
            let n = &mut s.nations[civ];
            n.members += 1;
            for r in pair {
                n.offices[r.index()] = id;
            }
        }
    }
}

/// Resolve one tick with the given orders per civ; check invariants.
/// Vacant offices get test officers first (`staff`).
pub fn step(s: &mut WorldState, rules: &Ruleset, orders: Vec<(u16, Vec<Order>)>) {
    staff(s);
    // Tests exercise mechanics, not office budgets (V5 §5.2 has its own tests).
    for n in &mut s.nations {
        n.role_bank = [20; 4];
    }
    let batches = orders
        .into_iter()
        .flat_map(|(civ, orders)| office_batches(s, civ, [9; 32], orders))
        .collect();
    resolve_tick(
        s,
        rules,
        &TickInput {
            vrf: vrf(s.tick),
            batches,
            ..Default::default()
        },
    )
    .unwrap();
    let v = invariants::check(s, rules);
    assert!(v.is_empty(), "{v:?}");
}
pub fn free(s: &WorldState, h: Hex) -> bool {
    s.map.tile(h).is_some_and(|t| t.terrain.is_passable())
        && !s.units.iter().any(|u| u.alive && u.hex == h)
        && !s.cities.iter().any(|c| c.alive && c.hex == h)
        && !s.city_states.iter().any(|c| c.hex == h)
}
