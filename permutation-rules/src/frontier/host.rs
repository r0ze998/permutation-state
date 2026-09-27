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
//! * So are the roster's **values**: a merge, split or Depart (its march
//!   stamina) issued during bell b is recorded as a [`Pending`] change and
//!   executed after the clash of b ([`Host::settle`]); garrison changes
//!   likewise ([`GarrisonState`]). The clash of b reads `values_at(b)`.

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
    /// A change is already pending for this bell (one per host per bell).
    Busy,
    /// A change issued during an earlier bell has not been settled: its
    /// bell's clash must be applied and [`Host::settle`] called first.
    Unsettled,
    /// The province has not resolved the bells this needs yet (lag only
    /// waits, design §8.4).
    Unresolved,
    /// A value was set for a bell before the one it already holds
    /// (`Stamina::set`, CL-05): a late write must never overwrite a later
    /// spend.
    TimeReversed,
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

    /// Set the value at resolved bell `b` (clash results, routs).
    /// Monotone in the bell (CL-05): refused for `b < self.bell`, so a set
    /// for an earlier bell can no longer overwrite a later spend. Setting
    /// at the stored bell is allowed (it replaces that bell's value).
    pub fn set(&mut self, b: u32, value: u16) -> Result<(), HostError> {
        if b < self.bell {
            return Err(HostError::TimeReversed);
        }
        *self = Stamina {
            value: value.min(STAMINA_CAP),
            bell: b,
        };
        Ok(())
    }
}

/// A change to a host issued during bell `bell` (design §6.3, roster
/// freeze). It takes effect at the start of `bell + 1`, **after** the clash
/// of `bell`: the clash of `bell` reads the host's values from before the
/// change, and the change is executed on the values the clash left
/// ([`Host::settle`]). So a Depart, a merge or a split issued after seeing
/// a revealed arrival never changes that bell's clash.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PendingOp {
    /// A Depart: the march's stamina, charged after the origin's clash
    /// (down to 0 if the clash spent more).
    Spend { cost: u16 },
    /// Split `troops` of the `of` troops the host had when the split was
    /// issued into host `new_id`: after the clash the new host gets the
    /// same share of the survivors.
    Split {
        troops: MilliTroops,
        of: MilliTroops,
        new_id: u64,
    },
    /// Absorb host `from` (same owner, faction and unit) after the clash.
    Absorb { from: u64 },
    /// Absorbed into host `into` after the clash (this host dissolves).
    AbsorbedInto { into: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Pending {
    /// The bell the change was issued in.
    pub bell: u32,
    pub op: PendingOp,
}

/// A host's rules-side state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Host {
    pub id: u64,
    /// Owning holding.
    pub owner: u64,
    pub faction: u8,
    pub unit: UnitType,
    /// Troops and stamina in force for every bell up to the clash of the
    /// pending change's bell (and after it, once there is no pending change).
    pub troops: MilliTroops,
    pub stamina: Stamina,
    /// First bell at which the host may depart again.
    pub ready_bell: u32,
    /// At most one change per host per bell, executed by [`Host::settle`].
    pub pending: Option<Pending>,
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
            pending: None,
        })
    }

    /// Troops and stamina the clash of bell `b` reads: the values in force
    /// at the start of `b`. A change issued during an earlier bell must be
    /// settled first ([`HostError::Unsettled`]).
    pub fn values_at(&self, b: u32) -> Result<(MilliTroops, u16), HostError> {
        if let Some(p) = self.pending {
            if b > p.bell {
                return Err(HostError::Unsettled);
            }
        }
        Ok((self.troops, self.stamina.at(b)))
    }

    /// Record a change issued during bell `b`. The host's province must be
    /// resolved through `b − 2`, the last bell whose reveal window can have
    /// closed (a bell resolves at the earliest one bell after it ends,
    /// §8.4; `resolved_next` is the first bell the province has not
    /// resolved). One change per host is pending at a time: an older one
    /// must have been settled ([`Host::settle`], [`settle_merge`]) so nothing
    /// it creates is lost, and one issued during `b − 1` makes the host wait
    /// a bell ([`HostError::Busy`]).
    fn check_issue(&self, b: u32, resolved_next: u32) -> Result<(), HostError> {
        if resolved_next.saturating_add(1) < b {
            return Err(HostError::Unresolved);
        }
        match self.pending {
            Some(p) if p.bell >= resolved_next => Err(HostError::Busy),
            Some(_) => Err(HostError::Unsettled),
            None => Ok(()),
        }
    }

    /// Merge `other` into `self`, issued during bell `b`: same owner, faction
    /// and unit type, at most 30,000 troops. Both keep fighting separately
    /// in the clash of `b`; [`settle_merge`] joins what survives: troops
    /// add up, stamina is the lower of the two survivors'.
    pub fn merge(&mut self, other: &mut Host, b: u32, resolved_next: u32) -> Result<(), HostError> {
        if self.owner != other.owner
            || self.unit != other.unit
            || self.faction != other.faction
            || self.id == other.id
        {
            return Err(HostError::Mismatch);
        }
        self.check_issue(b, resolved_next)?;
        other.check_issue(b, resolved_next)?;
        let t = self.troops as u64 + other.troops as u64;
        if t > MAX_HOST_TROOPS as u64 {
            return Err(HostError::TooLarge);
        }
        self.pending = Some(Pending {
            bell: b,
            op: PendingOp::Absorb { from: other.id },
        });
        other.pending = Some(Pending {
            bell: b,
            op: PendingOp::AbsorbedInto { into: self.id },
        });
        Ok(())
    }

    /// Split `troops` off into a new host `new_id`, issued during bell `b`;
    /// both keep ≥ 100 troops (checked arithmetic: no wrap in release
    /// builds). The new host is created by [`Host::settle`] after the
    /// clash of `b`, with its share of the survivors, and joins the roster
    /// at `b + 1`.
    pub fn split(
        &mut self,
        troops: MilliTroops,
        new_id: u64,
        b: u32,
        resolved_next: u32,
    ) -> Result<(), HostError> {
        self.check_issue(b, resolved_next)?;
        let room = self
            .troops
            .checked_sub(MIN_HOST_TROOPS)
            .ok_or(HostError::TooSmall)?;
        if troops < MIN_HOST_TROOPS || troops > room || troops > MAX_HOST_TROOPS {
            return Err(HostError::TooSmall);
        }
        self.pending = Some(Pending {
            bell: b,
            op: PendingOp::Split {
                troops,
                of: self.troops,
                new_id,
            },
        });
        Ok(())
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

    /// Depart during bell `b`: the host still fights in its origin's clash
    /// of `b` (it leaves the roster at `b + 1`) and pays the march's stamina
    /// after it.
    pub fn depart(
        &mut self,
        b: u32,
        stamina_cost: u16,
        resolved_next: u32,
    ) -> Result<(), HostError> {
        self.check_issue(b, resolved_next)?;
        self.check_depart(b, stamina_cost)?;
        self.pending = Some(Pending {
            bell: b,
            op: PendingOp::Spend { cost: stamina_cost },
        });
        Ok(())
    }

    /// Apply this host's result of the clash of bell `b` (troops and stamina
    /// after it). Results are applied in bell order, before the pending
    /// change of an earlier bell is settled.
    pub fn apply_clash(
        &mut self,
        b: u32,
        troops: MilliTroops,
        stamina: u16,
        engaged: bool,
    ) -> Result<(), HostError> {
        if let Some(p) = self.pending {
            if b > p.bell {
                return Err(HostError::Unsettled);
            }
        }
        // Checked before any write, so a refused result changes nothing.
        let mut st = self.stamina;
        st.set(b, stamina)?;
        self.troops = troops;
        self.stamina = st;
        if engaged {
            self.ready_bell = self.ready_bell.max(b + 1 + BATTLE_COOLDOWN_BELLS);
        }
        Ok(())
    }

    /// Execute the pending change once its bell's clash has been applied
    /// (`resolved_next` > its bell). Returns the host a split created.
    /// A merge is settled with [`settle_merge`], which needs both hosts.
    pub fn settle(&mut self, resolved_next: u32) -> Result<Option<Host>, HostError> {
        let Some(p) = self.pending else {
            return Ok(None);
        };
        if resolved_next <= p.bell {
            return Ok(None);
        }
        let e = effective_bell(p.bell);
        match p.op {
            PendingOp::Spend { cost } => {
                let v = self.stamina.at(e).saturating_sub(cost);
                self.stamina.set(e, v)?;
                self.pending = None;
                Ok(None)
            }
            PendingOp::Split { troops, of, new_id } => {
                let part = if of == 0 {
                    0
                } else {
                    (self.troops as u64 * troops as u64 / of as u64) as MilliTroops
                };
                self.troops -= part;
                self.pending = None;
                Ok(Some(Host {
                    id: new_id,
                    troops: part,
                    pending: None,
                    ..*self
                }))
            }
            PendingOp::Absorb { .. } | PendingOp::AbsorbedInto { .. } => Err(HostError::Busy),
        }
    }

    /// Troops and stamina of a host that departed during `depart_bell`, as
    /// its arrival reads them: the values after its origin's clash of the
    /// departure bell and the march's stamina. Reveal's quota ranking and
    /// the destination's clash wait (lag only waits, §8.4) until the origin
    /// has resolved through the departure bell.
    pub fn march_values(
        &self,
        depart_bell: u32,
        origin_resolved_next: u32,
    ) -> Result<(MilliTroops, u16), HostError> {
        if origin_resolved_next <= depart_bell {
            return Err(HostError::Unresolved);
        }
        if self.pending.is_some() {
            return Err(HostError::Unsettled);
        }
        Ok((self.troops, self.stamina.at(effective_bell(depart_bell))))
    }

    /// An unrevealed arrival: home with half its troops and no stamina.
    /// Refused (nothing changes) for a bell before its stamina's bell.
    pub fn route(&mut self, b: u32) -> Result<(), HostError> {
        self.stamina.set(b, 0)?;
        self.troops = rout_survivors(self.troops);
        Ok(())
    }

    /// Strength for `retreat_ratio` and holding the field.
    pub fn strength(&self) -> u64 {
        strength(self.unit, self.troops)
    }
}

/// Settle a merge issued during bell b once the clash of b has been
/// applied to both hosts: `into` takes the survivors of both (troops add
/// up; stamina is the lower of the two that still have troops) and `from`
/// is left with none (the program closes it).
pub fn settle_merge(into: &mut Host, from: &mut Host, resolved_next: u32) -> Result<(), HostError> {
    let (Some(a), Some(b)) = (into.pending, from.pending) else {
        return Err(HostError::Mismatch);
    };
    match (a.op, b.op) {
        (PendingOp::Absorb { from: f }, PendingOp::AbsorbedInto { into: i })
            if f == from.id && i == into.id && a.bell == b.bell => {}
        _ => return Err(HostError::Mismatch),
    }
    if resolved_next <= a.bell {
        return Err(HostError::Unresolved);
    }
    let e = effective_bell(a.bell);
    let alive = |h: &Host| h.troops >= DESTROYED_BELOW;
    let stamina = match (alive(into), alive(from)) {
        (true, true) => into.stamina.at(e).min(from.stamina.at(e)),
        (true, false) => into.stamina.at(e),
        (false, true) => from.stamina.at(e),
        (false, false) => 0,
    };
    let mut st = into.stamina;
    st.set(e, stamina)?;
    let t = into.troops as u64 + from.troops as u64;
    into.troops = t.min(MAX_HOST_TROOPS as u64) as MilliTroops;
    into.stamina = st;
    into.ready_bell = into.ready_bell.max(from.ready_bell);
    into.pending = None;
    from.troops = 0;
    from.pending = None;
    Ok(())
}

/// A holding's garrison as the clash reads it (design §6.3): garrison
/// changes issued during bell b (reinforcements, auto-reinforce transfers,
/// musters drawn from it) take effect at b + 1, after the clash of b, like
/// host changes. Changes may be issued while the province is resolved
/// through `b − 2`, so at most two bells (`b − 1` and `b`) have changes
/// pending; each slot sums the changes of one bell.
///
/// **Cap (CL-01, integ-W1):** a garrison never holds more than
/// [`MAX_HOST_TROOPS`], because `clash::validate` refuses a garrison above
/// it and a refused clash input would freeze the province. [`Self::change`]
/// refuses a delta that could take the garrison past the cap (counted as if
/// the pending clashes cost nothing), [`Self::room`] says how much a
/// program path that must not fail (a host returning home) may add, and
/// [`Self::settle`] clamps to the cap as a last guard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct GarrisonState {
    pub troops: MilliTroops,
    /// `(bell, net change)` of up to two unsettled bells.
    pub pending: [Option<(u32, i64)>; 2],
}

impl GarrisonState {
    /// A garrison of `troops`, clamped to [`MAX_HOST_TROOPS`].
    pub const fn new(troops: MilliTroops) -> GarrisonState {
        GarrisonState {
            troops: if troops > MAX_HOST_TROOPS {
                MAX_HOST_TROOPS
            } else {
                troops
            },
            pending: [None, None],
        }
    }

    /// The largest value the garrison can reach once every pending change
    /// is settled, if no clash costs it anything.
    fn projected(&self) -> i64 {
        self.pending
            .iter()
            .flatten()
            .fold(self.troops as i64, |t, (_, d)| t.saturating_add(*d))
    }

    /// Troops a change issued now may still add without passing
    /// [`MAX_HOST_TROOPS`] (0 when full).
    pub fn room(&self) -> MilliTroops {
        (MAX_HOST_TROOPS as i64 - self.projected()).clamp(0, MAX_HOST_TROOPS as i64) as MilliTroops
    }

    /// Troops the clash of bell `b` reads (the value at the start of `b`):
    /// every change of a bell before `b` must have been settled.
    pub fn at(&self, b: u32) -> Result<MilliTroops, HostError> {
        if self.pending.iter().flatten().any(|(pb, _)| *pb < b) {
            return Err(HostError::Unsettled);
        }
        Ok(self.troops)
    }

    /// A change of `delta` issued during bell `b` (the province resolved up
    /// to, not including, `resolved_next ≥ b − 1`). A positive delta above
    /// [`Self::room`] is refused with [`HostError::TooLarge`].
    pub fn change(&mut self, b: u32, delta: i64, resolved_next: u32) -> Result<(), HostError> {
        if resolved_next.saturating_add(1) < b {
            return Err(HostError::Unresolved);
        }
        self.settle(resolved_next);
        if delta > 0 && delta > self.room() as i64 {
            return Err(HostError::TooLarge);
        }
        if let Some(slot) = self.pending.iter_mut().flatten().find(|(pb, _)| *pb == b) {
            slot.1 = slot.1.checked_add(delta).ok_or(HostError::TooLarge)?;
            return Ok(());
        }
        match self.pending.iter_mut().find(|x| x.is_none()) {
            Some(free) => {
                *free = Some((b, delta));
                Ok(())
            }
            None => Err(HostError::Unsettled),
        }
    }

    /// The garrison's troops after the clash of bell `b`.
    pub fn apply_clash(&mut self, b: u32, troops: MilliTroops) -> Result<(), HostError> {
        self.at(b)?;
        self.troops = troops.min(MAX_HOST_TROOPS);
        Ok(())
    }

    /// Execute the pending changes whose bell's clash has been applied
    /// (`bell < resolved_next`), oldest first.
    pub fn settle(&mut self, resolved_next: u32) {
        self.pending.sort_by_key(|x| x.map_or(u32::MAX, |(b, _)| b));
        for x in self.pending.iter_mut() {
            if let Some((b, d)) = *x {
                if b < resolved_next {
                    let v = (self.troops as i64)
                        .saturating_add(d)
                        .clamp(0, MAX_HOST_TROOPS as i64);
                    self.troops = v as MilliTroops;
                    *x = None;
                }
            }
        }
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
