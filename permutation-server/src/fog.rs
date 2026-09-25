//! The information model for the server: **perfect information**.
//!
//! Every account of a season is public on chain (the world and nation
//! accounts on the ER can be read by anyone), so a fog applied by the server
//! would hide from people in the web client what an agent reading the chain
//! sees anyway. Every player — the human client, hosted AI members and the
//! scripted bots — therefore decides from the full state, and every view
//! shows it. What stays hidden is what the chain hides: an officer's sealed
//! orders until they are revealed.
//!
//! Vision (`vision::visible`) is kept for display only (`sight`): the map can
//! show what a nation's units and cities overlook, but nothing is withheld.
//! The rules crate still has the belief-state machinery (`vision::belief`)
//! for a possible fog mode enforced by a private rollup; the server does not
//! use it.

use std::borrow::Cow;

use permutation_rules::state::{CivId, WorldState};
use permutation_rules::vision::{visible, Memory};

pub struct Fog {
    /// Per civ, per tile: in sight of one of its (or its allies') units or
    /// cities. Display only.
    sight: Vec<Vec<bool>>,
    /// Every tile known: all `true`.
    all: Vec<bool>,
    /// A memory that has explored everything (the decision-log leaves, V5 D17,
    /// record the observation as the full state).
    memory: Memory,
}

impl Fog {
    /// As of `state` (call right after genesis).
    pub fn new(state: &WorldState) -> Fog {
        let n = state.map.tiles.len();
        let mut memory = Memory::new(state);
        memory.explored = vec![true; n];
        let mut f = Fog {
            sight: vec![Vec::new(); state.civs.len()],
            all: vec![true; n],
            memory,
        };
        f.update(state);
        f
    }

    /// Refresh the display sight after a tick resolves.
    pub fn update(&mut self, state: &WorldState) {
        for civ in 0..state.civs.len() {
            self.sight[civ] = visible(state, civ as CivId);
        }
    }

    /// Tiles `civ` knows: all of them.
    pub fn seen(&self, _civ: CivId) -> &[bool] {
        &self.all
    }

    pub fn memory(&self, _civ: CivId) -> &Memory {
        &self.memory
    }

    /// What `civ` decides from: the full state, borrowed (a fog mode would
    /// return an owned belief state).
    pub fn belief<'a>(&self, state: &'a WorldState, _civ: CivId) -> Cow<'a, WorldState> {
        Cow::Borrowed(state)
    }

    /// Per tile, for display: `2` in sight of `civ`, `1` known but out of sight.
    pub fn sight_code(&self, civ: CivId) -> String {
        self.sight[civ as usize]
            .iter()
            .map(|s| if *s { '2' } else { '1' })
            .collect()
    }
}
