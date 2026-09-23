//! Buildings (§5.6). Each building can be built once per city.

use crate::tech::Tech;
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize,
)]
pub enum Building {
    Granary,
    Workshop,
    Temple,
    Market,
    Academy,
    Barracks,
    Walls,
    StarGate1,
    StarGate2,
    StarGate3,
}

#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct BuildingInfo {
    pub building: Building,
    pub prod_cost: u32,
    pub tech: Option<Tech>,
}

use Building::*;

const fn b(building: Building, prod_cost: u32, tech: Option<Tech>) -> BuildingInfo {
    BuildingInfo {
        building,
        prod_cost,
        tech,
    }
}

/// Indexed by `Building as usize`.
pub const BUILDINGS: [BuildingInfo; 10] = [
    b(Granary, 40, None),
    b(Workshop, 40, None),
    b(Temple, 50, Some(Tech::Mysticism)),
    b(Market, 60, Some(Tech::Currency)),
    b(Academy, 60, Some(Tech::Writing)),
    b(Barracks, 50, Some(Tech::BronzeWorking)),
    b(Walls, 60, Some(Tech::Masonry)),
    b(StarGate1, 200, Some(Tech::Astronomy)),
    b(StarGate2, 300, Some(Tech::Physics)),
    b(StarGate3, 400, Some(Tech::CelestialMechanics)),
];

pub const fn info(building: Building) -> &'static BuildingInfo {
    &BUILDINGS[building as usize]
}

impl Building {
    pub const fn is_star_gate(self) -> bool {
        matches!(self, StarGate1 | StarGate2 | StarGate3)
    }
}

/// Buildings present in a city, as a bitmask.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BuildingSet(pub u16);

impl BuildingSet {
    pub const fn has(self, b: Building) -> bool {
        self.0 & (1 << b as u16) != 0
    }
    pub fn insert(&mut self, b: Building) {
        self.0 |= 1 << b as u16;
    }
    pub fn remove(&mut self, b: Building) {
        self.0 &= !(1 << b as u16);
    }
    pub fn star_gate_stages(self) -> u8 {
        [StarGate1, StarGate2, StarGate3]
            .iter()
            .filter(|b| self.has(**b))
            .count() as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_indexed_by_discriminant() {
        for (i, bi) in BUILDINGS.iter().enumerate() {
            assert_eq!(bi.building as usize, i);
        }
    }
}
