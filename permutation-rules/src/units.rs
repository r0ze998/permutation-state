//! Unit table (§7.1).

use crate::tech::Tech;
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize,
)]
pub enum UnitType {
    Spearman,
    Archer,
    Horseman,
    Pikeman,
    Crossbowman,
    Knight,
    Scout,
    Settler,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum UnitClass {
    Melee,
    Ranged,
    Mounted,
    Civilian,
}

#[derive(Clone, Copy, Debug, BorshSerialize, BorshDeserialize)]
pub struct UnitStats {
    pub unit: UnitType,
    pub class: UnitClass,
    pub tier: u8,
    /// Strength per troop.
    pub strength: u32,
    pub movement: u8,
    /// Production per troop (Settler: per unit, plus 1 pop).
    pub prod_cost: u32,
    pub iron: u32,
    pub horses: u32,
    pub range: u8,
    pub vision: u8,
    pub tech: Option<Tech>,
    /// Upkeep weight in bps: T2 troops count as 1.5 (§6.1).
    pub upkeep_bps: u32,
}

use UnitClass::*;
use UnitType::*;

#[allow(clippy::too_many_arguments)]
const fn u(
    unit: UnitType,
    class: UnitClass,
    tier: u8,
    strength: u32,
    movement: u8,
    prod_cost: u32,
    iron: u32,
    horses: u32,
    range: u8,
    vision: u8,
    tech: Option<Tech>,
) -> UnitStats {
    UnitStats {
        unit,
        class,
        tier,
        strength,
        movement,
        prod_cost,
        iron,
        horses,
        range,
        vision,
        tech,
        upkeep_bps: if tier == 2 { 15_000 } else { 10_000 },
    }
}

/// Indexed by `UnitType as usize`.
/// Columns: unit, class, tier, strength, move, prod, iron, horses, range, vision, tech.
#[rustfmt::skip]
pub const UNIT_STATS: [UnitStats; 8] = [
    u(Spearman,    Melee,    1, 10, 1,  6, 0, 0, 1, 2, None),
    u(Archer,      Ranged,   1, 10, 1,  7, 0, 0, 2, 2, Some(Tech::Archery)),
    u(Horseman,    Mounted,  1, 10, 2,  9, 0, 0, 1, 2, Some(Tech::HorsebackRiding)),
    u(Pikeman,     Melee,    2, 22, 1, 12, 1, 0, 1, 2, Some(Tech::IronWorking)),
    u(Crossbowman, Ranged,   2, 22, 1, 14, 1, 0, 2, 2, Some(Tech::Mathematics)),
    u(Knight,      Mounted,  2, 22, 2, 16, 0, 2, 1, 2, Some(Tech::Chivalry)),
    u(Scout,       Civilian, 0,  0, 3, 10, 0, 0, 0, 3, None),
    u(Settler,     Civilian, 0,  0, 1, 30, 0, 0, 0, 2, None),
];

pub const fn stats(unit: UnitType) -> &'static UnitStats {
    &UNIT_STATS[unit as usize]
}

impl UnitType {
    pub const fn is_civilian(self) -> bool {
        matches!(stats(self).class, Civilian)
    }
    pub const fn can_capture(self) -> bool {
        matches!(stats(self).class, Melee | Mounted)
    }
}

/// Counter relation (§7.1): `attacker` gets ×15000 against `defender`.
/// Melee counters mounted, ranged counters melee, mounted counters ranged.
pub const fn counters(attacker: UnitType, defender: UnitType) -> bool {
    matches!(
        (stats(attacker).class, stats(defender).class),
        (Melee, Mounted) | (Ranged, Melee) | (Mounted, Ranged)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_indexed_by_discriminant() {
        for (i, s) in UNIT_STATS.iter().enumerate() {
            assert_eq!(s.unit as usize, i);
        }
    }

    #[test]
    fn counter_triangle_matches_spec() {
        assert!(counters(Spearman, Horseman) && counters(Pikeman, Knight));
        assert!(counters(Archer, Spearman) && counters(Crossbowman, Pikeman));
        assert!(counters(Horseman, Archer) && counters(Knight, Crossbowman));
        assert!(!counters(Spearman, Archer) && !counters(Archer, Horseman));
        assert!(!counters(Spearman, Spearman));
    }
}
