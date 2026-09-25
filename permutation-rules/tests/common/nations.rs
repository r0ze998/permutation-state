//! Nations with members, offices and governance actions for tests.

use super::vrf;
use permutation_rules::genesis::{nation_entries, new_season};
use permutation_rules::gov::{self, GovAction, GovEntry, MemberId, Role};
use permutation_rules::invariants;
use permutation_rules::orders::{Order, OrderBatch};
use permutation_rules::state::WorldState;
use permutation_rules::tick::{resolve_tick, TickInput};
use permutation_rules::{Preset, Ruleset};

pub const DIGEST: [u8; 32] = [9; 32];

pub fn key(m: u32) -> [u8; 32] {
    let mut k = [0u8; 32];
    k[..4].copy_from_slice(&(m + 1).to_le_bytes());
    k
}

/// Six nations; `members[c]` members join nation `c` (ids in order).
pub fn world(members: &[usize]) -> (Ruleset, WorldState) {
    let rules = Ruleset::new(Preset::Blitz);
    let mut s = new_season(&rules, &[11; 32], &[22; 32], &nation_entries(6)).unwrap();
    let mut id = 0;
    for (civ, n) in members.iter().enumerate() {
        for _ in 0..*n {
            assert_eq!(gov::join(&mut s, &rules, civ as u16, key(id)).unwrap(), id);
            id += 1;
        }
    }
    (rules, s)
}

pub fn act(m: MemberId, action: GovAction) -> GovEntry {
    GovEntry {
        member: m,
        signer: key(m),
        action,
    }
}

/// Member `m` stands for `roles`, and everyone in its nation votes for it.
pub fn elect(s: &WorldState, entries: &mut Vec<GovEntry>, m: MemberId, roles: &[Role]) {
    let mask = roles.iter().fold(0, |a, r| a | r.bit());
    entries.push(act(m, GovAction::Stand { roles: mask }));
    let civ = s.members[m as usize].civ;
    for (v, x) in s.members.iter().enumerate() {
        if x.civ == civ {
            for r in roles {
                entries.push(act(
                    v as u32,
                    GovAction::Vote {
                        role: *r,
                        candidate: m,
                    },
                ));
            }
        }
    }
}

/// One office's batch from a member (or the acting official).
pub fn batch(
    s: &WorldState,
    civ: u16,
    role: Role,
    member: MemberId,
    orders: Vec<Order>,
) -> OrderBatch {
    OrderBatch {
        civ,
        tick: s.tick,
        role,
        member,
        decision_digest: DIGEST,
        orders,
        adopt: vec![],
    }
}

pub fn step(s: &mut WorldState, rules: &Ruleset, batches: Vec<OrderBatch>, gov: Vec<GovEntry>) {
    let before = s.clone();
    resolve_tick(
        s,
        rules,
        &TickInput {
            vrf: vrf(s.tick),
            batches,
            gov,
            deposits: vec![],
        },
    )
    .unwrap();
    let v = invariants::check(s, rules);
    assert!(v.is_empty(), "{v:?}");
    assert!(invariants::check_monotonic(&before, s).is_empty());
}

pub fn idle(s: &mut WorldState, rules: &Ruleset, ticks: u16) {
    for _ in 0..ticks {
        step(s, rules, vec![], vec![]);
    }
}
