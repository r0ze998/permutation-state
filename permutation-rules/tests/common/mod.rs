//! Helpers shared by the integration tests (`mod common;`).
#![allow(dead_code)] // each test crate uses a subset

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
/// Resolve one tick with the given orders per civ; check invariants.
pub fn step(s: &mut WorldState, rules: &Ruleset, orders: Vec<(u16, Vec<Order>)>) {
    // Tests exercise mechanics, not office budgets (V5 §5.2 has its own tests).
    for n in &mut s.nations {
        n.role_bank = [20; 4];
    }
    let batches = orders
        .into_iter()
        .flat_map(|(civ, orders)| office_batches(s, civ, [0; 32], orders))
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
