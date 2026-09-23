//! Technology tree (§6.2). Technologies are never transferable.

use borsh::{BorshDeserialize, BorshSerialize};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize,
)]
pub enum Tech {
    Agriculture,
    BronzeWorking,
    Archery,
    HorsebackRiding,
    Masonry,
    Mysticism,
    Writing,
    Currency,
    IronWorking,
    Mathematics,
    Chivalry,
    Philosophy,
    Engineering,
    Astronomy,
    Physics,
    CelestialMechanics,
}

pub const TECH_COUNT: usize = 16;

#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct TechInfo {
    pub tech: Tech,
    pub era: u8,
    pub base_cost: u32,
    pub prereqs: [Option<Tech>; 2],
}

use Tech::*;

const fn t(tech: Tech, era: u8, base_cost: u32, a: Option<Tech>, b: Option<Tech>) -> TechInfo {
    TechInfo {
        tech,
        era,
        base_cost,
        prereqs: [a, b],
    }
}

/// Indexed by `Tech as usize`.
pub const TECHS: [TechInfo; TECH_COUNT] = [
    t(Agriculture, 1, 40, None, None),
    t(BronzeWorking, 1, 40, None, None),
    t(Archery, 1, 40, None, None),
    t(HorsebackRiding, 1, 40, None, None),
    t(Masonry, 2, 90, Some(BronzeWorking), None),
    t(Mysticism, 2, 90, Some(Agriculture), None),
    t(Writing, 2, 90, Some(Agriculture), None),
    t(Currency, 2, 90, Some(BronzeWorking), None),
    t(IronWorking, 2, 90, Some(BronzeWorking), None),
    t(Mathematics, 3, 160, Some(Writing), Some(Currency)),
    t(Chivalry, 3, 160, Some(HorsebackRiding), Some(IronWorking)),
    t(Philosophy, 3, 160, Some(Mysticism), Some(Writing)),
    t(Engineering, 3, 160, Some(Masonry), Some(Mathematics)),
    t(Astronomy, 4, 260, Some(Mathematics), Some(Philosophy)),
    t(Physics, 4, 320, Some(Astronomy), None),
    t(CelestialMechanics, 4, 400, Some(Physics), None),
];

pub const fn info(tech: Tech) -> &'static TechInfo {
    &TECHS[tech as usize]
}

/// Set of researched technologies as a bitmask.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TechSet(pub u32);

impl TechSet {
    pub const fn has(self, tech: Tech) -> bool {
        self.0 & (1 << tech as u32) != 0
    }
    pub fn insert(&mut self, tech: Tech) {
        self.0 |= 1 << tech as u32;
    }
    pub fn count(self) -> u32 {
        self.0.count_ones()
    }
    /// Invariant 10 (§17): prerequisites are always held.
    pub fn prereqs_met(self, tech: Tech) -> bool {
        info(tech).prereqs.iter().flatten().all(|p| self.has(*p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_indexed_by_discriminant() {
        for (i, ti) in TECHS.iter().enumerate() {
            assert_eq!(ti.tech as usize, i);
        }
    }

    #[test]
    fn tree_is_acyclic_and_never_looks_forward() {
        // Every prerequisite appears earlier in the table and in the same or an
        // earlier era, so the table order is a valid research order.
        for (i, ti) in TECHS.iter().enumerate() {
            for p in ti.prereqs.iter().flatten() {
                assert!((*p as usize) < i, "{:?} requires later {:?}", ti.tech, p);
                assert!(info(*p).era <= ti.era);
            }
        }
    }

    #[test]
    fn minimum_star_gate_path_costs_1650() {
        let path = [
            Agriculture,
            BronzeWorking,
            Writing,
            Mysticism,
            Currency,
            Mathematics,
            Philosophy,
            Astronomy,
            Physics,
            CelestialMechanics,
        ];
        let mut set = TechSet::default();
        let mut total = 0;
        for tech in path {
            assert!(set.prereqs_met(tech), "{tech:?}");
            set.insert(tech);
            total += info(tech).base_cost;
        }
        assert_eq!(total, 1650);
    }
}
