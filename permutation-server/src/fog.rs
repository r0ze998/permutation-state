//! Per-civilization fog of war for the server: memory and belief states.
//!
//! Every player — the human client and every bot — decides from
//! `Fog::belief`, never from the full state. Orders are still validated and
//! resolved on the full state by the engine.

use permutation_rules::state::{CivId, WorldState};
use permutation_rules::vision::{belief, sharers, visible, Memory};

pub struct Fog {
    seen: Vec<Vec<bool>>,
    memory: Vec<Memory>,
}

impl Fog {
    /// Fog as of `state` (call right after genesis).
    pub fn new(state: &WorldState) -> Fog {
        let n = state.civs.len();
        let mut f = Fog {
            seen: vec![Vec::new(); n],
            memory: vec![Memory::new(state); n],
        };
        f.update(state);
        f
    }

    /// Refresh vision and memory after a tick resolves.
    pub fn update(&mut self, state: &WorldState) {
        for civ in 0..state.civs.len() {
            let seen = visible(state, civ as CivId);
            self.memory[civ].update(state, civ as CivId, &seen);
            self.seen[civ] = seen;
        }
    }

    pub fn seen(&self, civ: CivId) -> &[bool] {
        &self.seen[civ as usize]
    }

    pub fn memory(&self, civ: CivId) -> &Memory {
        &self.memory[civ as usize]
    }

    pub fn belief(&self, state: &WorldState, civ: CivId) -> WorldState {
        belief(state, civ, self.seen(civ), self.memory(civ))
    }

    /// Per tile: `0` never seen, `1` remembered, `2` in sight.
    pub fn code(&self, civ: CivId) -> String {
        let m = self.memory(civ);
        self.seen(civ)
            .iter()
            .zip(&m.explored)
            .map(|(s, e)| {
                if *s {
                    '2'
                } else if *e {
                    '1'
                } else {
                    '0'
                }
            })
            .collect()
    }
}

/// Whether a chronicle line (`kind|text`) is public to `viewer`.
/// Research is private to a civ and its allies; everything else in the
/// chronicle is a public announcement (wars, treaties, captures, Star Gates).
pub fn line_is_public(state: &WorldState, viewer: CivId, line: &str, names: &[&str]) -> bool {
    let Some(text) = line.strip_prefix("tech|") else {
        return true;
    };
    let team = sharers(state, viewer);
    team.iter().any(|c| {
        names
            .get(*c as usize)
            .is_some_and(|n| text.starts_with(&format!("{n} ")))
    })
}
