//! Hosts (design §6.1): armies of one unit type on the map.
//!
//! * Up to 30,000 troops of one of the unit types of `units` except
//!   Settlers; at least 100 troops when mustered.
//! * Stamina, cap 120, refilled lazily by 1 per **resolved** bell of the
//!   province the host is in (a stalled province is a paused province,
//!   design §8.4), and a battle cooldown.
//! * ≤ 6 hosts per hex, ≤ 48 per province, ≤ 8 residents per faction per
//!   province, 4 transit slots per holding.
//! * Roster membership is by bell (`Presence`): anything issued during
//!   bell b takes effect at the start of b + 1, so the roster of bell b is
//!   frozen at its start (design §6.3).

use crate::fixed::{Bps, MilliTroops, BPS_ONE, MILLI};
use crate::units::{stats, UnitType};
use borsh::{BorshDeserialize, BorshSerialize};

/// Largest host: 30,000 troops.
pub const MAX_HOST_TROOPS: MilliTroops = 30_000 * MILLI as MilliTroops;
/// Smallest host at muster (and after a split): 100 troops.
pub const MIN_HOST_TROOPS: MilliTroops = 100 * MILLI as MilliTroops;
/// Hosts below half a troop are destroyed.
pub const DESTROYED_BELOW: MilliTroops = 500;
pub const STAMINA_CAP: u16 = 120;
/// Stamina an engaging host pays (refunded if its side's damage ratio is
/// ≥ 10) [design].
pub const ENGAGE_STAMINA: u16 = 20;
/// Bells a host that fought cannot depart after the clash.
pub const BATTLE_COOLDOWN_BELLS: u32 = 1;
pub const HEX_HOST_CAP: usize = 6;
/// On a holding's hex the owner faction always has this many slots, and
/// all other factions together at most this many.
pub const OWNER_HEX_SLOTS: usize = 3;
pub const PROVINCE_HOST_CAP: usize = 48;
pub const FACTION_RESIDENT_CAP: usize = 8;
pub const TRANSIT_SLOTS: usize = 4;
/// An arrival not revealed by the reveal close is routed: it goes home and
/// loses this share of its troops, its stamina and its tip (design §6.2).
pub const ROUT_LOSS_BPS: Bps = 5_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostError {
    /// Settlers never form hosts.
    NotAHostUnit,
    TooSmall,
    TooLarge,
    /// Merging hosts of different types or owners.
    Mismatch,
    NoStamina,
    /// The host fought this bell or the last and cannot depart yet.
    Cooldown,
}

/// Lazy stamina: `value` at resolved bell `bell`, +1 per resolved bell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Stamina {
    pub value: u16,
    pub bell: u32,
}

impl Stamina {
    pub const fn full(bell: u32) -> Stamina {
        Stamina {
            value: STAMINA_CAP,
            bell,
        }
    }

    /// Stamina at resolved bell `b` (never below the stored value).
    pub const fn at(self, b: u32) -> u16 {
        if b <= self.bell {
            return self.value;
        }
        let v = self.value as u64 + (b - self.bell) as u64;
        if v > STAMINA_CAP as u64 {
            STAMINA_CAP
        } else {
            v as u16
        }
    }

    /// Refill to `b`, then spend `cost`.
    pub fn spend(&mut self, b: u32, cost: u16) -> Result<(), HostError> {
        let v = self.at(b);
        if v < cost {
            return Err(HostError::NoStamina);
        }
        *self = Stamina {
            value: v - cost,
            bell: b.max(self.bell),
        };
        Ok(())
    }

    /// Refill to `b`, then set (clash results, routs).
    pub fn set(&mut self, b: u32, value: u16) {
        *self = Stamina {
            value: value.min(STAMINA_CAP),
            bell: b.max(self.bell),
        };
    }
}

/// A host's rules-side state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Host {
    pub id: u64,
    /// Owning holding.
    pub owner: u64,
    pub faction: u8,
    pub unit: UnitType,
    pub troops: MilliTroops,
    pub stamina: Stamina,
    /// First bell at which the host may depart again.
    pub ready_bell: u32,
}

/// Whether `unit` can form a host (every unit type except Settlers).
pub const fn host_unit(unit: UnitType) -> bool {
    !matches!(unit, UnitType::Settler)
}

impl Host {
    /// Muster at bell `b` (joins the roster at `b + 1`) with full stamina.
    pub fn muster(
        id: u64,
        owner: u64,
        faction: u8,
        unit: UnitType,
        troops: MilliTroops,
        b: u32,
    ) -> Result<Host, HostError> {
        if !host_unit(unit) {
            return Err(HostError::NotAHostUnit);
        }
        if troops < MIN_HOST_TROOPS {
            return Err(HostError::TooSmall);
        }
        if troops > MAX_HOST_TROOPS {
            return Err(HostError::TooLarge);
        }
        Ok(Host {
            id,
            owner,
            faction,
            unit,
            troops,
            stamina: Stamina::full(b),
            ready_bell: b,
        })
    }

    /// Merge `other` into `self` at bell `b`: same owner and unit type, at
    /// most 30,000 troops; stamina is the lower of the two.
    pub fn merge(&mut self, other: &Host, b: u32) -> Result<(), HostError> {
        if self.owner != other.owner || self.unit != other.unit || self.faction != other.faction {
            return Err(HostError::Mismatch);
        }
        let t = self.troops as u64 + other.troops as u64;
        if t > MAX_HOST_TROOPS as u64 {
            return Err(HostError::TooLarge);
        }
        self.troops = t as MilliTroops;
        let s = self.stamina.at(b).min(other.stamina.at(b));
        self.stamina.set(b, s);
        self.ready_bell = self.ready_bell.max(other.ready_bell);
        Ok(())
    }

    /// Split `troops` off into a new host `new_id`; both keep ≥ 100 troops.
    pub fn split(&mut self, troops: MilliTroops, new_id: u64) -> Result<Host, HostError> {
        if troops < MIN_HOST_TROOPS || self.troops < troops + MIN_HOST_TROOPS {
            return Err(HostError::TooSmall);
        }
        self.troops -= troops;
        Ok(Host {
            id: new_id,
            troops,
            ..*self
        })
    }

    /// Whether the host may depart at bell `b`.
    pub fn check_depart(&self, b: u32, stamina_cost: u16) -> Result<(), HostError> {
        if b < self.ready_bell {
            return Err(HostError::Cooldown);
        }
        if self.stamina.at(b) < stamina_cost {
            return Err(HostError::NoStamina);
        }
        Ok(())
    }

    /// An unrevealed arrival: home with half its troops and no stamina.
    pub fn route(&mut self, b: u32) {
        self.troops = rout_survivors(self.troops);
        self.stamina.set(b, 0);
    }

    /// Strength for `retreat_ratio` and holding the field.
    pub fn strength(&self) -> u64 {
        strength(self.unit, self.troops)
    }
}

/// Troops left after a rout.
pub const fn rout_survivors(troops: MilliTroops) -> MilliTroops {
    (troops as u64 * (BPS_ONE - ROUT_LOSS_BPS) as u64 / BPS_ONE as u64) as MilliTroops
}

/// Strength of a force: troops × per-troop strength (milli-units).
pub const fn strength(unit: UnitType, troops: MilliTroops) -> u64 {
    troops as u64 * stats(unit).strength as u64
}

/// Strength of a holding's garrison fighting as `Combatant::City`
/// (strength 10 per troop, `combat`).
pub const fn city_strength(garrison: MilliTroops) -> u64 {
    garrison as u64 * 10
}

/// Supply attrition (design §3.6): a host more than 3 provinces from any
/// friendly holding loses 1% of its troops per bell, compounded; closed
/// form over `bells`, truncating once at the end. Apply it from a fixed
/// anchor (troops and bell when the host left supply), never chained per
/// touch, so the result does not depend on when the host is touched.
pub fn supply_attrition(troops: MilliTroops, bells: u32) -> MilliTroops {
    const ONE: u128 = 1 << 64;
    let mut factor: u128 = ONE;
    let mut base: u128 = ONE * 99 / 100;
    let mut n = bells;
    while n > 0 {
        if n & 1 == 1 {
            factor = (factor * base) >> 64;
        }
        base = (base * base) >> 64;
        n >>= 1;
    }
    ((troops as u128 * factor) >> 64) as MilliTroops
}

/// A roster entry's bells: in the roster of bell b iff
/// `from_bell ≤ b < until_bell`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Presence {
    pub from_bell: u32,
    /// `u32::MAX`: still present.
    pub until_bell: u32,
}

impl Presence {
    /// Present from the bell after `issued_bell` (Muster, arrival merged at
    /// the clash of `issued_bell`, garrison change).
    pub const fn from_next(issued_bell: u32) -> Presence {
        Presence {
            from_bell: effective_bell(issued_bell),
            until_bell: u32::MAX,
        }
    }
    pub const fn in_roster(self, b: u32) -> bool {
        self.from_bell <= b && b < self.until_bell
    }
    /// Leave (Depart, Dissolve, garrison change) issued during `issued_bell`:
    /// still in that bell's roster, gone from the next.
    pub fn leave(&mut self, issued_bell: u32) {
        self.until_bell = self.until_bell.min(effective_bell(issued_bell));
    }
}

/// An action issued during bell b takes effect at the start of b + 1.
pub const fn effective_bell(issued_bell: u32) -> u32 {
    issued_bell.saturating_add(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamina_refills_by_resolved_bell() {
        let mut s = Stamina {
            value: 10,
            bell: 100,
        };
        assert_eq!(s.at(100), 10);
        assert_eq!(s.at(150), 60);
        assert_eq!(s.at(1_000), STAMINA_CAP);
        s.spend(150, 60).unwrap();
        assert_eq!(s.at(150), 0);
        assert!(s.spend(150, 1).is_err());
    }

    #[test]
    fn attrition_compounds() {
        let t = 1_000_000;
        assert_eq!(supply_attrition(t, 0), t);
        assert_eq!(supply_attrition(t, 1), 990_000 - 1); // truncation
        let a = supply_attrition(t, 69);
        assert!((499_000..=500_500).contains(&a), "{a}"); // 0.99^69 ≈ 0.4998
    }
}
