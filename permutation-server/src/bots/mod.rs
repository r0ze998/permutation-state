//! Scripted rule-based bots: the hosted AI members' planner and the sim's
//! players. They play through the same order API as a person, decide from
//! the full state like every player (perfect information), and commit a
//! short rationale each tick (§4.3).
//!
//! A bot's temperament (`Traits`) is drawn per season from a secret on the
//! operator's server (V5 §18.8) and picks its persona (Warlord, Builder,
//! Diplomat, Scholar); `persona_of` is the fixed layout the golden test,
//! replay and ticklog use.
//!
//! - `plan`: the orders of one tick, step by step
//! - `contracts`: treasury contracts (V5 §18.6)
//! - `geo`: paths, sites and counts on the map
//! - `rationale`: the sealed rationale text

use permutation_rules::buildings::Building;
use permutation_rules::state::CivId;
use permutation_rules::tech::Tech;
use std::collections::HashMap;

mod contracts;
mod geo;
mod plan;
mod rationale;

pub use geo::{city_count, troops_of};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Persona {
    Warlord,
    Builder,
    Diplomat,
    Scholar,
}

impl Persona {
    pub fn name(self) -> &'static str {
        match self {
            Persona::Warlord => "Warlord",
            Persona::Builder => "Builder",
            Persona::Diplomat => "Diplomat",
            Persona::Scholar => "Scholar",
        }
    }
    fn target_cities(self) -> usize {
        match self {
            Persona::Warlord => 4,
            Persona::Builder => 6,
            Persona::Diplomat => 5,
            Persona::Scholar => 4,
        }
    }
    fn research(self) -> &'static [Tech] {
        use Tech::*;
        match self {
            Persona::Warlord => &[
                BronzeWorking,
                Archery,
                HorsebackRiding,
                IronWorking,
                Agriculture,
                Masonry,
                Chivalry,
                Currency,
                Writing,
                Mathematics,
                Mysticism,
                Philosophy,
                Engineering,
            ],
            Persona::Builder => &[
                Agriculture,
                BronzeWorking,
                Currency,
                Writing,
                Masonry,
                Mysticism,
                Mathematics,
                IronWorking,
                Philosophy,
                Engineering,
                Astronomy,
                Physics,
                CelestialMechanics,
            ],
            Persona::Diplomat => &[
                Agriculture,
                Mysticism,
                Writing,
                BronzeWorking,
                Currency,
                Philosophy,
                Masonry,
                Archery,
                Mathematics,
                Astronomy,
                IronWorking,
                Physics,
                CelestialMechanics,
            ],
            Persona::Scholar => &[
                Agriculture,
                Writing,
                BronzeWorking,
                Currency,
                Mysticism,
                Mathematics,
                Philosophy,
                Astronomy,
                Physics,
                CelestialMechanics,
                Archery,
                Masonry,
                IronWorking,
            ],
        }
    }
    fn buildings(self) -> &'static [Building] {
        use Building::*;
        match self {
            Persona::Warlord => &[Barracks, Granary, Workshop, Walls, Market, Temple, Academy],
            Persona::Builder => &[Granary, Workshop, Market, Academy, Temple, Walls, Barracks],
            Persona::Diplomat => &[Granary, Temple, Workshop, Academy, Market, Walls, Barracks],
            Persona::Scholar => &[Granary, Academy, Workshop, Temple, Market, Walls, Barracks],
        }
    }
}

/// The fixed persona of nation `civ` (seats beyond six reuse the list): the
/// layout of the golden test, replay and ticklog. Live seasons draw
/// temperaments instead (`Traits::draw`).
pub fn persona_of(civ: CivId) -> Persona {
    PERSONAS[civ as usize % PERSONAS.len()]
}

pub const PERSONAS: [Persona; 6] = [
    Persona::Warlord,
    Persona::Builder,
    Persona::Diplomat,
    Persona::Scholar,
    Persona::Warlord,
    Persona::Diplomat,
];

/// An AI's temperament (V5 §18.8), 0..=100 each, drawn per season on the

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Traits {
    /// Willingness to fight and to pay others to fight.
    pub aggression: u8,
    /// How much USDC a deal must bring before it is taken.
    pub greed: u8,
    /// Keeps its word (pacts, alliances) rather than selling it.
    pub loyalty: u8,
    /// Answers offers and talk at all.
    pub openness: u8,
}

impl Traits {
    /// The traits a persona has when nothing was drawn (sim, local tests).
    pub fn of(p: Persona) -> Traits {
        let (aggression, greed, loyalty, openness) = match p {
            Persona::Warlord => (80, 60, 40, 40),
            Persona::Builder => (30, 50, 60, 60),
            Persona::Diplomat => (20, 40, 70, 90),
            Persona::Scholar => (25, 45, 60, 50),
        };
        Traits {
            aggression,
            greed,
            loyalty,
            openness,
        }
    }

    /// Traits drawn from `seed` for nation `civ`.
    pub fn draw(seed: &[u8], civ: CivId) -> Traits {
        use sha2::{Digest, Sha256};
        let h: [u8; 32] = Sha256::new()
            .chain_update(b"PS/traits")
            .chain_update(seed)
            .chain_update(civ.to_le_bytes())
            .finalize()
            .into();
        Traits {
            aggression: h[0] % 101,
            greed: h[1] % 101,
            loyalty: h[2] % 101,
            openness: h[3] % 101,
        }
    }

    /// The persona that plays these traits.
    pub fn persona(self) -> Persona {
        if self.aggression >= 60 {
            Persona::Warlord
        } else if self.openness >= 60 {
            Persona::Diplomat
        } else if self.greed >= 55 {
            Persona::Builder
        } else {
            Persona::Scholar
        }
    }

    /// Least USDC (base units) a contract must pay before this AI takes it.
    pub fn price(self) -> u64 {
        500_000 + self.greed as u64 * 40_000
    }
}

/// A scripted rule-based player. Reads the full state, like every player.
pub struct Bot {
    pub civ: CivId,
    pub persona: Persona,
    pub traits: Traits,
    pub war_started: HashMap<CivId, u16>,
    /// Cities this bot marches on first when at war (sim only: the
    /// operator AIs' home cities, to measure what a leak is worth, V5 §18).
    /// Hosted bots never get any.
    pub targets: Vec<permutation_rules::state::CityId>,
}

impl Bot {
    pub fn new(civ: CivId, persona: Persona) -> Self {
        Bot {
            civ,
            persona,
            traits: Traits::of(persona),
            war_started: HashMap::new(),
            targets: Vec::new(),
        }
    }

    /// A bot whose persona and traits were drawn for the season.
    pub fn with_traits(civ: CivId, traits: Traits) -> Self {
        Bot {
            civ,
            persona: traits.persona(),
            traits,
            war_started: HashMap::new(),
            targets: Vec::new(),
        }
    }
}
